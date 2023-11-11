use std::{str::FromStr, sync::Arc};

use anyhow::{anyhow, Context};
use openidconnect::{
    core::{CoreClient, CoreIdToken, CoreProviderMetadata, CoreUserInfoClaims},
    reqwest::async_http_client,
    AccessToken, EndSessionUrl, LogoutRequest, PostLogoutRedirectUrl, ProviderMetadataWithLogout,
    SubjectIdentifier, TokenIntrospectionResponse,
};
use sqlx::{Postgres, Transaction};
use tracing::info;

use universal_inbox::{
    auth::AuthIdToken,
    user::{AuthUserId, User, UserId},
};
use url::Url;

use crate::{
    configuration::ApplicationSettings,
    repository::user::UserRepository,
    repository::Repository,
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

#[derive(Debug)]
pub struct UserService {
    repository: Arc<Repository>,
    application_settings: ApplicationSettings,
}

impl UserService {
    pub fn new(
        repository: Arc<Repository>,
        application_settings: ApplicationSettings,
    ) -> UserService {
        UserService {
            repository,
            application_settings,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn get_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: UserId,
    ) -> Result<Option<User>, UniversalInboxError> {
        self.repository.get_user(executor, id).await
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn fetch_all_users<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
    ) -> Result<Vec<User>, UniversalInboxError> {
        self.repository.fetch_all_users(executor).await
    }

    #[tracing::instrument(level = "debug", skip(self, executor, auth_id_token), err)]
    pub async fn authenticate_and_create_user_if_not_exists<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        access_token: &str,
        auth_id_token: AuthIdToken,
    ) -> Result<User, UniversalInboxError> {
        let provider_metadata = CoreProviderMetadata::discover_async(
            self.application_settings
                .security
                .authentication
                .oidc_issuer_url
                .clone(),
            async_http_client,
        )
        .await
        .context("metadata provider error")?;

        // Create an OpenID Connect client by specifying the client ID
        let client = CoreClient::from_provider_metadata(
            provider_metadata,
            self.application_settings
                .security
                .authentication
                .oidc_api_client_id
                .clone(),
            Some(
                self.application_settings
                    .security
                    .authentication
                    .oidc_api_client_secret
                    .clone(),
            ),
        )
        .set_introspection_uri(
            self.application_settings
                .security
                .authentication
                .oidc_introspection_url
                .clone(),
        );

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
            .update_user_auth_id_token(executor, &auth_user_id, &auth_id_token)
            .await?
        {
            UpdateStatus {
                updated: _,
                result: Some(user),
            } => Ok(user),
            UpdateStatus {
                updated: _,
                result: None,
            } => {
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
                        User::new(auth_user_id, auth_id_token, first_name, last_name, email),
                    )
                    .await
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self, executor), err)]
    pub async fn close_session<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Url, UniversalInboxError> {
        let provider_metadata = ProviderMetadataWithLogout::discover_async(
            self.application_settings
                .security
                .authentication
                .oidc_issuer_url
                .clone(),
            async_http_client,
        )
        .await
        .context("metadata provider error")?;
        let end_session_url: EndSessionUrl = provider_metadata
            .additional_metadata()
            .end_session_endpoint
            .as_ref()
            .ok_or_else(|| anyhow!("No end session endpoint found in provider metadata"))?
            .clone();
        let logout_request: LogoutRequest = end_session_url.into();

        let user = self
            .repository
            .get_user(executor, user_id)
            .await?
            .ok_or_else(|| anyhow!("User with ID {user_id} not found"))?;
        let id_token = CoreIdToken::from_str(&user.auth_id_token.to_string())
            .context("Could not parse stored OIDC ID token, this should not happen")?;

        Ok(logout_request
            .set_id_token_hint(&id_token)
            .set_post_logout_redirect_uri(PostLogoutRedirectUrl::from_url(
                self.application_settings.front_base_url.clone(),
            ))
            .http_get_url())
    }
}
