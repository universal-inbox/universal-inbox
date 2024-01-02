use std::fmt::Debug;

use anyhow::Context;
use async_trait::async_trait;
use enum_display::EnumDisplay;
use lettre::{
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use mailgen::{themes::DefaultTheme, Action, Branding, Email, EmailBuilder, Greeting, Mailgen};
use secrecy::{ExposeSecret, Secret};
use serde::Serialize;
use tracing::info;
use url::Url;

use universal_inbox::user::User;

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
        first_name: String,
        email_verification_url: Url,
    },
}

impl EmailTemplate {
    pub fn subject(&self) -> String {
        match self {
            EmailTemplate::EmailVerification { .. } => "Verify your email".to_string(),
        }
    }

    pub fn build_email_body(&self) -> Email {
        match self {
            EmailTemplate::EmailVerification {
                first_name,
                email_verification_url,
            } => EmailBuilder::new()
                .greeting(Greeting::Name(first_name))
                .intro("Please verify your email address to start using Universal Inbox")
                .action(Action {
                    text: "Verify your email",
                    link: email_verification_url.as_str(),
                    color: Some(("#388FEF", "white")),
                    ..Default::default()
                })
                .outro("Welcome to Universal Inbox")
                .signature("Best")
                .build(),
        }
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
        smtp_password: Secret<String>,
        from_header: Mailbox,
        reply_to_header: Mailbox,
    ) -> Result<Self, UniversalInboxError> {
        let creds = Credentials::new(smtp_username, smtp_password.expose_secret().to_string());

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

    #[tracing::instrument(level = "debug", skip(self), err)]
    fn build_email(
        &self,
        user: User,
        template: EmailTemplate,
    ) -> Result<Message, UniversalInboxError> {
        let theme = DefaultTheme::new();
        let branding = Branding {
            logo: Some(
                "https://www.universal-inbox.com/images/ui-logo-transparent.png".to_string(),
            ),
            ..Branding::new("Universal Inbox", "https://www.universal-inbox.com")
        };
        let email_body = template.build_email_body();
        let mailgen = Mailgen::new(theme, branding);

        let email_txt_body = mailgen
            .render_text(&email_body)
            .context("Failed to render email as text")?;
        let email_html_body = mailgen
            .render_html(&email_body)
            .context("Failed to render email as HTML")?;

        Ok(Message::builder()
            .from(self.from_header.clone())
            .reply_to(self.reply_to_header.clone())
            .to(
                format!("{} {} <{}>", user.first_name, user.last_name, user.email)
                    .parse()
                    .context("Failed to parse user email `to` header")?,
            )
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
    #[tracing::instrument(level = "info", skip(self), err)]
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
