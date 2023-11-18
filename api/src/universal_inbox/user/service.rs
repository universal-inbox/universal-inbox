use std::{str::FromStr, sync::Arc};

use anyhow::{anyhow, Context};
use openidconnect::{
    core::{
        CoreGenderClaim, CoreIdToken, CoreJsonWebKeyType, CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm, CoreUserInfoClaims,
    },
    reqwest::async_http_client,
    AccessToken, AuthorizationCode, CsrfToken, EmptyAdditionalClaims, EndSessionUrl, IdToken,
    LogoutRequest, Nonce, PostLogoutRedirectUrl, ProviderMetadataWithLogout, RedirectUrl,
    SubjectIdentifier, TokenIntrospectionResponse,
};
use sqlx::{Postgres, Transaction};
use tracing::info;
use url::Url;

use universal_inbox::{
    auth::openidconnect::OpenidConnectProvider,
    user::{AuthUserId, User, UserId},
};

use crate::{
    configuration::{ApplicationSettings, OIDCFlowSettings},
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

    #[tracing::instrument(level = "debug", skip(self, executor, nonce), err)]
    pub async fn authenticate_for_auth_code_flow<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        code: AuthorizationCode,
        nonce: Nonce,
    ) -> Result<User, UniversalInboxError> {
        let (access_token, id_token) = self.fetch_access_token(code, nonce.clone()).await?;

        // In the Authorization code flow, the API has fetched the access token and thus does not need to
        // validate it.
        let oidc_provider = self.get_openid_connect_provider().await?;
        let auth_user_id = oidc_provider
            .verify_id_token_claims(&id_token, &nonce)?
            .subject()
            .to_string()
            .into();

        self.authenticate_and_create_user_if_not_exists(
            executor,
            oidc_provider,
            access_token,
            id_token,
            auth_user_id,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip(self, oidc_provider), err)]
    async fn verify_access_token(
        &self,
        oidc_provider: &mut OpenidConnectProvider,
        access_token: &AccessToken,
    ) -> Result<AuthUserId, UniversalInboxError> {
        let OIDCFlowSettings::AuthorizationCodePKCEFlow {
            introspection_url, ..
        } = &self
            .application_settings
            .security
            .authentication
            .oidc_flow_settings
        else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "Cannot validate OIDC ID Token without a configured introspection URL"
            )));
        };
        // The introspection URL is only used for the Authorization code PKCE flow as
        // the API server must validate (ie. has not be revoked) the access token sent by the front.
        let client = oidc_provider
            .client
            .clone()
            .set_introspection_uri(introspection_url.clone());

        let introspection_result = client
            .introspect(access_token)
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

        Ok(introspection_result
            .sub()
            .ok_or_else(|| anyhow!("No subject found in introspection result"))?
            .to_string()
            .into())
    }

    #[tracing::instrument(level = "debug", skip(self, executor, id_token), err)]
    pub async fn authenticate_for_auth_code_pkce_flow<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        access_token: AccessToken,
        id_token: IdToken<
            EmptyAdditionalClaims,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
            CoreJsonWebKeyType,
        >,
    ) -> Result<User, UniversalInboxError> {
        let mut oidc_provider = self.get_openid_connect_provider().await?;
        let auth_user_id: AuthUserId = self
            .verify_access_token(&mut oidc_provider, &access_token)
            .await?;
        self.authenticate_and_create_user_if_not_exists(
            executor,
            oidc_provider,
            access_token,
            id_token,
            auth_user_id,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip(self, executor, oidc_provider, id_token), err)]
    async fn authenticate_and_create_user_if_not_exists<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        oidc_provider: OpenidConnectProvider,
        // the access token must have been validated before calling this function
        access_token: AccessToken,
        id_token: IdToken<
            EmptyAdditionalClaims,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
            CoreJsonWebKeyType,
        >,
        auth_user_id: AuthUserId,
    ) -> Result<User, UniversalInboxError> {
        match self
            .repository
            .update_user_auth_id_token(executor, &auth_user_id, &id_token.to_string().into())
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
                let user_infos: CoreUserInfoClaims = oidc_provider
                    .client
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
                        User::new(
                            auth_user_id,
                            id_token.to_string().into(),
                            first_name,
                            last_name,
                            email,
                        ),
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
        match &self
            .application_settings
            .security
            .authentication
            .oidc_flow_settings
        {
            OIDCFlowSettings::AuthorizationCodePKCEFlow { .. } => {
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
            OIDCFlowSettings::GoogleAuthorizationCodeFlow => Ok(
                "https://accounts.google.com/logout?continue=http://localhost:8000"
                    .parse::<Url>()
                    .unwrap(),
            ),
        }
    }

    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn build_auth_url<'a>(&self) -> Result<(Url, CsrfToken, Nonce), UniversalInboxError> {
        Ok(self
            .get_openid_connect_provider()
            .await?
            .build_google_authorization_code_flow_auth_url())
    }

    #[allow(clippy::type_complexity)]
    #[tracing::instrument(level = "debug", skip(self), err)]
    pub async fn fetch_access_token<'a>(
        &self,
        auth_code: AuthorizationCode,
        nonce: Nonce,
    ) -> Result<
        (
            AccessToken,
            IdToken<
                EmptyAdditionalClaims,
                CoreGenderClaim,
                CoreJweContentEncryptionAlgorithm,
                CoreJwsSigningAlgorithm,
                CoreJsonWebKeyType,
            >,
        ),
        UniversalInboxError,
    > {
        Ok(self
            .get_openid_connect_provider()
            .await?
            .fetch_access_token(auth_code, nonce, None)
            .await?)
    }

    async fn get_openid_connect_provider(
        &self,
    ) -> Result<OpenidConnectProvider, UniversalInboxError> {
        let redirect_url = RedirectUrl::new(
            self.application_settings
                .get_oidc_auth_code_flow_redirect_url()?
                .to_string(),
        )
        .context("Failed to build OpenID connect redirection URL from {redirect_url}")?;

        Ok(OpenidConnectProvider::build(
            self.application_settings
                .security
                .authentication
                .oidc_issuer_url
                .clone(),
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
            redirect_url,
        )
        .await?)
    }
}
