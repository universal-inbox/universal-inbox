use std::fmt::Debug;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use chrono::NaiveDate;
use enum_display::EnumDisplay;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
};
use mailgen::{Action, Branding, Email, EmailBuilder, Greeting, Mailgen, themes::DefaultTheme};
use secrecy::{ExposeSecret, SecretBox};
use serde::Serialize;
use tracing::info;
use url::Url;

use universal_inbox::{integration_connection::IntegrationConnectionPausedReason, user::User};

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
    IntegrationConnectionPauseWarning {
        first_name: Option<String>,
        provider_name: String,
        inactive_for_days: i64,
        pause_date: NaiveDate,
        app_url: Url,
    },
    IntegrationConnectionPaused {
        first_name: Option<String>,
        provider_name: String,
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
            EmailTemplate::IntegrationConnectionPauseWarning { provider_name, .. } => {
                format!("Your {provider_name} connection will soon be paused")
            }
            EmailTemplate::IntegrationConnectionPaused { provider_name, .. } => {
                format!("Your {provider_name} connection was paused")
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
                provider_name,
                inactive_for_days,
                pause_date,
                ..
            } => format!(
                "You haven't used Universal Inbox for more than {inactive_for_days} days. To stop collecting your {provider_name} data while you're away, your {provider_name} connection will be paused on {}. Open Universal Inbox before then to keep it connected.",
                pause_date.format("%B %-d, %Y")
            ),
            EmailTemplate::IntegrationConnectionPaused {
                provider_name,
                paused_reason: IntegrationConnectionPausedReason::Inactivity,
                ..
            } => format!(
                "Your {provider_name} connection was paused because you haven't used Universal Inbox for a while, and its access to {provider_name} was revoked. You can reconnect it at any time from the settings page."
            ),
            EmailTemplate::IntegrationConnectionPaused {
                provider_name,
                paused_reason: IntegrationConnectionPausedReason::LongFailing,
                ..
            } => format!(
                "Your {provider_name} connection was paused because it has been failing to synchronize for too long, and its access to {provider_name} was revoked. You can reconnect it at any time from the settings page."
            ),
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
        ),
        err
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
        let to = if let Some(first_name) = user.first_name {
            if let Some(last_name) = user.last_name {
                format!("{} {} <{}>", first_name, last_name, email)
                    .parse()
                    .context("Failed to parse user email `to` header")?
            } else {
                email
                    .to_string()
                    .parse()
                    .context("Failed to parse user email `to` header")?
            }
        } else {
            email
                .to_string()
                .parse()
                .context("Failed to parse user email `to` header")?
        };

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
        fields({ attr::USER_ID } = user.id.to_string(), { attr::EMAIL_SUBJECT } = template.subject()),
        err
    )]
    async fn send_email(
        &self,
        user: User,
        template: EmailTemplate,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::*;

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
            provider_name: "Slack".to_string(),
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
            provider_name: "Slack".to_string(),
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
}
