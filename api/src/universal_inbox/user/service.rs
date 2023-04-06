use std::sync::Arc;

use anyhow::{anyhow, Context};
use openidconnect::{
    core::{CoreClient, CoreProviderMetadata, CoreUserInfoClaims},
    reqwest::async_http_client,
    AccessToken, SubjectIdentifier, TokenIntrospectionResponse,
};
use sqlx::{Postgres, Transaction};
use tracing::info;

use universal_inbox::user::{AuthUserId, User, UserId};

use crate::{
    configuration::AuthenticationSettings, repository::user::UserRepository,
    repository::Repository, universal_inbox::UniversalInboxError,
};

pub struct UserService {
    repository: Arc<Repository>,
    authentication_settings: AuthenticationSettings,
}

impl UserService {
    pub fn new(
        repository: Arc<Repository>,
        authentication_settings: AuthenticationSettings,
    ) -> UserService {
        UserService {
            repository,
            authentication_settings,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: UserId,
    ) -> Result<Option<User>, UniversalInboxError> {
        self.repository.get_user(executor, id).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn fetch_all_users<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
    ) -> Result<Vec<User>, UniversalInboxError> {
        self.repository.fetch_all_users(executor).await
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn authenticate_and_create_user_if_not_exists<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        access_token: &str,
    ) -> Result<User, UniversalInboxError> {
        let provider_metadata = CoreProviderMetadata::discover_async(
            self.authentication_settings.oidc_issuer_url.clone(),
            async_http_client,
        )
        .await
        .context("metadata provider error")?;

        // Create an OpenID Connect client by specifying the client ID
        let client = CoreClient::from_provider_metadata(
            provider_metadata,
            self.authentication_settings.oidc_api_client_id.clone(),
            Some(self.authentication_settings.oidc_api_client_secret.clone()),
        )
        .set_introspection_uri(self.authentication_settings.oidc_introspection_url.clone());

        let access_token = AccessToken::new(access_token.to_string());
        let introspection_result = client
            .introspect(&access_token)
            .context("Introspection configuration error")?
            .set_token_type_hint("access_token")
            .request_async(async_http_client)
            .await
            .context("Introspection request error")?;

        if !introspection_result.active() {
            return Err(UniversalInboxError::Unauthorized(
                "Given access token is not active".to_string(),
            ));
        }

        let auth_user_id: AuthUserId = introspection_result
            .sub()
            .ok_or_else(|| anyhow!("No subject found in introspection result"))?
            .to_string()
            .into();

        match self
            .repository
            .get_user_by_auth_id(executor, auth_user_id.clone())
            .await?
        {
            Some(user) => Ok(user),
            None => {
                info!("User with auth provider user ID {auth_user_id} does not exists, creating a new one");
                let user_infos: CoreUserInfoClaims = client
                    .user_info(
                        access_token,
                        Some(SubjectIdentifier::new(auth_user_id.to_string())),
                    )
                    .context("UserInfo configuration error")?
                    .request_async(async_http_client)
                    .await
                    .context("UserInfo request error")?;

                let first_name = user_infos
                    .given_name()
                    .context("No given name found in user info")?
                    .get(None)
                    .context("No given name found in user info")?
                    .to_string();
                let last_name = user_infos
                    .family_name()
                    .context("No family name found in user info")?
                    .get(None)
                    .context("No family name found in user info")?
                    .to_string();
                let email = user_infos
                    .email()
                    .context("No email found in user info")?
                    .to_string();

                self.repository
                    .create_user(
                        executor,
                        User::new(auth_user_id, first_name, last_name, email),
                    )
                    .await
            }
        }
    }
}
