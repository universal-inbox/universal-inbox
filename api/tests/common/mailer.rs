use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use universal_inbox::user::User;

use universal_inbox_api::{
    mailer::{EmailTemplate, Mailer},
    universal_inbox::UniversalInboxError,
};

#[derive(Debug)]
pub struct MailerStub {
    pub emails_sent: Arc<RwLock<Vec<(User, EmailTemplate)>>>,
}

impl MailerStub {
    pub fn new() -> Self {
        Self {
            emails_sent: Arc::new(RwLock::new(vec![])),
        }
    }
}

#[async_trait]
impl Mailer for MailerStub {
    async fn send_email(
        &self,
        user: User,
        template: EmailTemplate,
        _dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        self.emails_sent.write().await.push((user, template));
        Ok(())
    }
}
