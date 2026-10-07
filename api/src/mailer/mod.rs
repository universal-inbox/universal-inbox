use std::fmt::Debug;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::NaiveDate;
use email_address::EmailAddress;
use enum_display::EnumDisplay;
use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
};
use mailgen::{Action, Branding, Email, EmailBuilder, Greeting, Mailgen, themes::DefaultTheme};
use secrecy::{ExposeSecret, SecretBox};
use serde::Serialize;
use tracing::{info, warn};
use universal_inbox::pii::Pii;
use url::Url;

use universal_inbox::{integration_connection::IntegrationConnectionPausedReason, user::User};

use crate::observability::RecordSpanError;
use crate::observability::attr;
use crate::universal_inbox::UniversalInboxError;

#[async_trait]
pub trait Mailer {
    async fn send_email(
        &self,
        user: User,
        template: EmailTemplate,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError>;

    /// False when no email can be sent (no email settings configured): the
    /// flows that cannot work without email degrade instead of calling
    /// `send_email`.
    fn is_enabled(&self) -> bool {
        true
    }
}

/// Mailer used when no email (SMTP) settings are configured: every email is
/// dropped with a warning.
pub struct DisabledMailer;

#[async_trait]
impl Mailer for DisabledMailer {
    async fn send_email(
        &self,
        user: User,
        template: EmailTemplate,
        _dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        warn!(
            "Email is disabled, not sending {template} email to user {}",
            user.id
        );
        Ok(())
    }

    fn is_enabled(&self) -> bool {
        false
    }
}

#[derive(Serialize, Debug, PartialEq, Clone, EnumDisplay)]
#[enum_display(case = "Snake")]
#[serde(untagged)]
pub enum EmailTemplate {
    EmailVerification {
        first_name: Option<String>,
        email_verification_url: Url,
    },
    PasswordReset {
        first_name: Option<String>,
        password_reset_url: Url,
    },
    RegistrationAttemptOnExistingAccount {
        first_name: Option<String>,
        login_url: Url,
        password_reset_url: Url,
    },
    AccountLockout {
        first_name: Option<String>,
        login_url: Url,
    },
    PasswordChanged {
        first_name: Option<String>,
        password_reset_url: Url,
    },
    /// Sent once per user, listing all their connections to be paused.
    IntegrationConnectionPauseWarning {
        first_name: Option<String>,
        provider_names: Vec<String>,
        inactive_for_days: i64,
        pause_date: NaiveDate,
        app_url: Url,
    },
    /// Sent once per user and reason, listing all their paused connections.
    IntegrationConnectionPaused {
        first_name: Option<String>,
        provider_names: Vec<String>,
        paused_reason: IntegrationConnectionPausedReason,
        reconnect_url: Url,
    },
}

impl EmailTemplate {
    pub fn subject(&self) -> String {
        match self {
            EmailTemplate::EmailVerification { .. } => "Verify your email".to_string(),
            EmailTemplate::PasswordReset { .. } => "Reset your password".to_string(),
            EmailTemplate::RegistrationAttemptOnExistingAccount { .. } => {
                "Someone tried to create an account with your email".to_string()
            }
            EmailTemplate::AccountLockout { .. } => {
                "Your Universal Inbox account was temporarily locked".to_string()
            }
            EmailTemplate::PasswordChanged { .. } => {
                "Your Universal Inbox password was changed".to_string()
            }
            EmailTemplate::IntegrationConnectionPauseWarning { provider_names, .. } => {
                let (connections, _) = connections_of(provider_names);
                format!("Your {connections} will soon be paused")
            }
            EmailTemplate::IntegrationConnectionPaused { provider_names, .. } => {
                let (connections, is_plural) = connections_of(provider_names);
                let verb = if is_plural { "were" } else { "was" };
                format!("Your {connections} {verb} paused")
            }
        }
    }

    fn first_name(&self) -> Option<&str> {
        match self {
            EmailTemplate::EmailVerification { first_name, .. }
            | EmailTemplate::PasswordReset { first_name, .. }
            | EmailTemplate::RegistrationAttemptOnExistingAccount { first_name, .. }
            | EmailTemplate::AccountLockout { first_name, .. }
            | EmailTemplate::PasswordChanged { first_name, .. }
            | EmailTemplate::IntegrationConnectionPauseWarning { first_name, .. }
            | EmailTemplate::IntegrationConnectionPaused { first_name, .. } => {
                first_name.as_deref()
            }
        }
    }

    fn intro(&self) -> String {
        match self {
            EmailTemplate::EmailVerification { .. } => {
                "Please verify your email address to start using Universal Inbox".to_string()
            }
            EmailTemplate::PasswordReset { .. } => "Reset your Universal Inbox password".to_string(),
            EmailTemplate::RegistrationAttemptOnExistingAccount { .. } => {
                "Someone tried to create a Universal Inbox account using your email address. If this was you, you can log in below. If you need to reset your password, use the \"Forgot password\" link on the login page. If this wasn't you, you can safely ignore this email, your account is secure.".to_string()
            }
            EmailTemplate::AccountLockout { .. } => {
                "Your Universal Inbox account was temporarily locked after too many failed login attempts. It will unlock automatically shortly. If this was you, simply try again later. If this wasn't you, someone may be trying to access your account — we recommend resetting your password using the \"Forgot password\" link on the login page.".to_string()
            }
            EmailTemplate::PasswordChanged { .. } => {
                "The password of your Universal Inbox account was just changed, and your other sessions were signed out. If this was you, there is nothing else to do. If this wasn't you, reset your password right away.".to_string()
            }
            EmailTemplate::IntegrationConnectionPauseWarning {
                provider_names,
                inactive_for_days,
                pause_date,
                ..
            } => {
                let providers = join_provider_names(provider_names);
                let (connections, is_plural) = connections_of(provider_names);
                let them = if is_plural { "them" } else { "it" };
                format!(
                    "You haven't used Universal Inbox for more than {inactive_for_days} days. To stop collecting your {providers} data while you're away, your {connections} will be paused on {}. Open Universal Inbox before then to keep {them} connected.",
                    pause_date.format("%B %-d, %Y")
                )
            }
            EmailTemplate::IntegrationConnectionPaused {
                provider_names,
                paused_reason,
                ..
            } => {
                let providers = join_provider_names(provider_names);
                let (connections, is_plural) = connections_of(provider_names);
                let (verb, they_have, their, them) = if is_plural {
                    ("were", "they have", "their", "them")
                } else {
                    ("was", "it has", "its", "it")
                };
                let because = match paused_reason {
                    IntegrationConnectionPausedReason::Inactivity => {
                        "you haven't used Universal Inbox for a while".to_string()
                    }
                    IntegrationConnectionPausedReason::LongFailing => {
                        format!("{they_have} been failing to synchronize for too long")
                    }
                };
                format!(
                    "Your {connections} {verb} paused because {because}, and {their} access to {providers} was revoked. You can reconnect {them} at any time from the settings page."
                )
            }
        }
    }

    fn action(&self) -> (&'static str, &Url) {
        match self {
            EmailTemplate::EmailVerification {
                email_verification_url,
                ..
            } => ("Verify your email", email_verification_url),
            EmailTemplate::PasswordReset {
                password_reset_url, ..
            } => ("Reset your password", password_reset_url),
            EmailTemplate::RegistrationAttemptOnExistingAccount { login_url, .. } => {
                ("Log in", login_url)
            }
            EmailTemplate::AccountLockout { login_url, .. } => ("Go to login", login_url),
            EmailTemplate::PasswordChanged {
                password_reset_url, ..
            } => ("Reset your password", password_reset_url),
            EmailTemplate::IntegrationConnectionPauseWarning { app_url, .. } => {
                ("Open Universal Inbox", app_url)
            }
            EmailTemplate::IntegrationConnectionPaused { reconnect_url, .. } => {
                ("Reconnect", reconnect_url)
            }
        }
    }

    fn outro(&self) -> Option<&'static str> {
        match self {
            EmailTemplate::EmailVerification { .. } => Some("Welcome to Universal Inbox"),
            _ => None,
        }
    }

    /// `intro` is [`Self::intro`], passed in because the rendered email
    /// borrows it.
    pub fn build_email_body<'a>(&'a self, intro: &'a str) -> Email<'a> {
        let mut builder = EmailBuilder::new();
        if let Some(first_name) = self.first_name() {
            builder = builder.greeting(Greeting::Name(first_name));
        }
        let (action_text, action_link) = self.action();
        builder = builder.intro(intro).action(Action {
            text: action_text,
            link: action_link.as_str(),
            color: Some(("#388FEF", "white")),
            ..Default::default()
        });
        if let Some(outro) = self.outro() {
            builder = builder.outro(outro);
        }

        builder.signature("Best").build()
    }
}

/// `["Slack", "Linear", "GitHub"]` reads "Slack, Linear and GitHub".
fn join_provider_names(provider_names: &[String]) -> String {
    match provider_names {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// "Slack connection", or "Slack and Linear connections", and whether it is
/// plural.
fn connections_of(provider_names: &[String]) -> (String, bool) {
    let is_plural = provider_names.len() > 1;
    let noun = if is_plural {
        "connections"
    } else {
        "connection"
    };
    (
        format!("{} {noun}", join_provider_names(provider_names)),
        is_plural,
    )
}

pub struct SmtpMailer {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from_header: Mailbox,
    reply_to_header: Mailbox,
}

impl SmtpMailer {
    pub fn build(
        smtp_server: String,
        smtp_port: u16,
        smtp_username: String,
        smtp_password: SecretBox<crate::configuration::SmtpPassword>,
        from_header: Mailbox,
        reply_to_header: Mailbox,
    ) -> Result<Self, UniversalInboxError> {
        let creds = Credentials::new(smtp_username, smtp_password.expose_secret().0.clone());

        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_server)
            .with_context(|| format!("Failed to connect to SMTP server {smtp_server}"))?
            .credentials(creds)
            .port(smtp_port)
            .build();

        Ok(Self {
            mailer,
            from_header,
            reply_to_header,
        })
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::USER_ID } = user.id.to_string(),
            { attr::EMAIL_SUBJECT } = template.subject(),
        )
    )]
    fn build_email(
        &self,
        user: User,
        template: EmailTemplate,
    ) -> Result<Message, UniversalInboxError> {
        let email = user.email.ok_or_else(|| {
            anyhow!(
                "Failed to build email for user {} without an email address",
                user.id
            )
        })?;
        let theme = DefaultTheme::new().context("Failed to create default theme")?;
        let branding = Branding {
            logo: Some(
                "https://www.universal-inbox.com/images/ui-logo-transparent.png".to_string(),
            ),
            ..Branding::new("Universal Inbox", "https://www.universal-inbox.com")
        };
        let intro = template.intro();
        let email_body = template.build_email_body(&intro);
        let mailgen = Mailgen::new(theme, branding);

        let email_txt_body = mailgen
            .render_text(&email_body)
            .context("Failed to render email as text")?;
        let email_html_body = mailgen
            .render_html(&email_body)
            .context("Failed to render email as HTML")?;
        let to = build_to_mailbox(user.first_name, user.last_name, &email)?;

        Ok(Message::builder()
            .from(self.from_header.clone())
            .reply_to(self.reply_to_header.clone())
            .to(to)
            .subject(template.subject())
            .multipart(MultiPart::alternative_plain_html(
                email_txt_body,
                email_html_body,
            ))
            .context("Failed to build email")?)
    }
}

#[async_trait]
impl Mailer for SmtpMailer {
    #[allow(clippy::blocks_in_conditions)]
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields({ attr::USER_ID } = user.id.to_string(), { attr::EMAIL_SUBJECT } = template.subject(), { attr::ERROR_TYPE } = tracing::field::Empty)
    )]
    async fn send_email(
        &self,
        user: User,
        template: EmailTemplate,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        let result: Result<(), UniversalInboxError> = async move {
            let email = self.build_email(user, template.clone())?;

            if dry_run {
                let email_file = format!("{template}.html");
                info!("[dry run] Writing email to send in {email_file}");
                std::fs::write(
                    email_file.clone(),
                    String::from_utf8(email.formatted()).unwrap(),
                )
                .with_context(|| format!("Failed to write email to {email_file}"))?;
            } else {
                self.mailer
                    .send(email)
                    .await
                    .context("Failed to send email")?;
            }

            Ok(())
        }
        .await;
        result.record_span_error()
    }
}

/// Builds the `To` mailbox from the user's name and address. The display name
/// goes through lettre's `Mailbox`, which quotes and encodes it, so it is never
/// parsed as part of the address.
fn build_to_mailbox(
    first_name: Option<String>,
    last_name: Option<String>,
    email: &Pii<EmailAddress>,
) -> Result<Mailbox, UniversalInboxError> {
    let address: Address = email
        .expose()
        .as_str()
        .parse()
        .context("Failed to parse user email address")?;
    let name = [first_name, last_name]
        .into_iter()
        .flatten()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    Ok(Mailbox::new((!name.is_empty()).then_some(name), address))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::*;

    #[rstest]
    #[case::full_name(Some("John"), Some("Doe"), "John Doe <john@example.com>")]
    #[case::first_name_only(Some("John"), None, "John <john@example.com>")]
    #[case::no_name(None, None, "john@example.com")]
    #[case::blank_name(Some(" "), None, "john@example.com")]
    #[case::special_chars(
        Some("Doe, John"),
        Some("<evil@example.com>"),
        "\"Doe, John <evil@example.com>\" <john@example.com>"
    )]
    fn test_build_to_mailbox(
        #[case] first_name: Option<&str>,
        #[case] last_name: Option<&str>,
        #[case] expected: &str,
    ) {
        let email: Pii<EmailAddress> = "john@example.com".parse().unwrap();
        let mailbox = build_to_mailbox(
            first_name.map(str::to_string),
            last_name.map(str::to_string),
            &email,
        )
        .unwrap();

        assert_eq!(mailbox.email.to_string(), "john@example.com");
        assert_eq!(mailbox.to_string(), expected);
    }

    fn render(template: &EmailTemplate) -> String {
        let mailgen = Mailgen::new(
            DefaultTheme::new().unwrap(),
            Branding::new("Universal Inbox", "https://www.universal-inbox.com"),
        );
        let intro = template.intro();
        mailgen
            .render_text(&template.build_email_body(&intro))
            .unwrap()
    }

    #[rstest]
    fn test_integration_connection_pause_warning_email() {
        let template = EmailTemplate::IntegrationConnectionPauseWarning {
            first_name: Some("John".to_string()),
            provider_names: vec!["Slack".to_string()],
            inactive_for_days: 83,
            pause_date: NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(),
            app_url: "https://app.universal-inbox.com/".parse().unwrap(),
        };

        assert_eq!(
            template.subject(),
            "Your Slack connection will soon be paused"
        );
        let intro = template.intro();
        assert!(intro.contains("for more than 83 days"), "{intro}");
        assert!(intro.contains("paused on October 10, 2026"), "{intro}");
        let text = render(&template);
        assert!(text.contains("John"), "{text}");
        assert!(text.contains("https://app.universal-inbox.com/"), "{text}");
    }

    #[rstest]
    fn test_integration_connection_emails_list_every_provider() {
        let provider_names = vec![
            "Slack".to_string(),
            "Linear".to_string(),
            "GitHub".to_string(),
        ];
        let warning = EmailTemplate::IntegrationConnectionPauseWarning {
            first_name: None,
            provider_names: provider_names.clone(),
            inactive_for_days: 83,
            pause_date: NaiveDate::from_ymd_opt(2026, 10, 10).unwrap(),
            app_url: "https://app.universal-inbox.com/".parse().unwrap(),
        };
        assert_eq!(
            warning.subject(),
            "Your Slack, Linear and GitHub connections will soon be paused"
        );
        let intro = warning.intro();
        assert!(intro.contains("keep them connected"), "{intro}");

        let paused = EmailTemplate::IntegrationConnectionPaused {
            first_name: None,
            provider_names,
            paused_reason: IntegrationConnectionPausedReason::LongFailing,
            reconnect_url: "https://app.universal-inbox.com/settings".parse().unwrap(),
        };
        assert_eq!(
            paused.subject(),
            "Your Slack, Linear and GitHub connections were paused"
        );
        let intro = paused.intro();
        assert!(
            intro.contains("they have been failing to synchronize"),
            "{intro}"
        );
        assert!(
            intro.contains("their access to Slack, Linear and GitHub was revoked"),
            "{intro}"
        );
    }

    #[rstest]
    #[case::inactivity(IntegrationConnectionPausedReason::Inactivity, "used Universal Inbox")]
    #[case::long_failing(
        IntegrationConnectionPausedReason::LongFailing,
        "failing to synchronize"
    )]
    fn test_integration_connection_paused_email(
        #[case] paused_reason: IntegrationConnectionPausedReason,
        #[case] expected_reason_text: &str,
    ) {
        let template = EmailTemplate::IntegrationConnectionPaused {
            first_name: None,
            provider_names: vec!["Slack".to_string()],
            paused_reason,
            reconnect_url: "https://app.universal-inbox.com/settings".parse().unwrap(),
        };

        assert_eq!(template.subject(), "Your Slack connection was paused");
        let intro = template.intro();
        assert!(intro.contains(expected_reason_text), "{intro}");
        let text = render(&template);
        assert!(text.contains("Reconnect"), "{text}");
        assert!(
            text.contains("https://app.universal-inbox.com/settings"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn test_disabled_mailer_drops_emails() {
        let mailer = DisabledMailer;
        let user = User::new(None, None, Pii::new("john@example.com".parse().unwrap()));
        let template = EmailTemplate::PasswordReset {
            first_name: None,
            password_reset_url: "https://app.universal-inbox.com/reset".parse().unwrap(),
        };

        assert!(!mailer.is_enabled());
        assert!(mailer.send_email(user, template, false).await.is_ok());
    }
}
