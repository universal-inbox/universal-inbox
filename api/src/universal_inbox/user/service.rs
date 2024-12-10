use std::{str::FromStr, sync::Arc};

use anyhow::{anyhow, Context};
use argon2::{password_hash::SaltString, Argon2, Params, PasswordHasher, PasswordVerifier};
use chrono::Utc;
use email_address::EmailAddress;
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
use secrecy::{ExposeSecret, Secret};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use url::Url;
use uuid::Uuid;

use universal_inbox::{
    auth::openidconnect::OpenidConnectProvider,
    user::{
        AuthUserId, Credentials, EmailValidationToken, LocalUserAuth, OpenIdConnectUserAuth,
        Password, PasswordHash, PasswordResetToken, User, UserAuth, UserId,
    },
};

use crate::{
    configuration::{
        ApplicationSettings, AuthenticationSettings, OIDCAuthorizationCodePKCEFlowSettings,
        OIDCFlowSettings, OpenIDConnectSettings,
    },
    mailer::{EmailTemplate, Mailer},
    observability::spawn_blocking_with_tracing,
    repository::user::UserRepository,
    repository::Repository,
    universal_inbox::{UniversalInboxError, UpdateStatus},
};

pub struct UserService {
    repository: Arc<Repository>,
    application_settings: ApplicationSettings,
    mailer: Arc<RwLock<dyn Mailer + Send + Sync>>,
}

impl UserService {
    pub fn new(
        repository: Arc<Repository>,
        application_settings: ApplicationSettings,
        mailer: Arc<RwLock<dyn Mailer + Send + Sync>>,
    ) -> UserService {
        UserService {
            repository,
            application_settings,
            mailer,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    pub async fn get_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        id: UserId,
    ) -> Result<Option<User>, UniversalInboxError> {
        self.repository.get_user(executor, id).await
    }

    pub async fn get_user_by_email<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        email: &EmailAddress,
    ) -> Result<Option<User>, UniversalInboxError> {
        self.repository.get_user_by_email(executor, email).await
    }

    pub async fn fetch_all_users<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
    ) -> Result<Vec<User>, UniversalInboxError> {
        self.repository.fetch_all_users(executor).await
    }

    /// In an OpenID Connect Authorization code flow, the API has fetched the access token and
    /// thus does not need to validate it.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn authenticate_for_auth_code_flow<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        openid_connect_settings: &OpenIDConnectSettings,
        code: AuthorizationCode,
        nonce: Nonce,
    ) -> Result<User, UniversalInboxError> {
        let (access_token, id_token) = self
            .fetch_access_token(openid_connect_settings, code, nonce.clone())
            .await?;

        let oidc_provider = self
            .get_openid_connect_provider(openid_connect_settings)
            .await?;
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

    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn verify_access_token(
        &self,
        pkce_flow_settings: &OIDCAuthorizationCodePKCEFlowSettings,
        oidc_provider: &mut OpenidConnectProvider,
        access_token: &AccessToken,
    ) -> Result<AuthUserId, UniversalInboxError> {
        // The introspection URL is only used for the Authorization code PKCE flow as
        // the API server must validate (ie. has not be revoked) the access token sent by the front.
        let client = oidc_provider
            .client
            .clone()
            .set_introspection_uri(pkce_flow_settings.introspection_url.clone());

        let introspection_result = client
            .introspect(access_token)
            .context("Introspection configuration error")?
            .set_token_type_hint("access_token")
            .request_async(async_http_client)
            .await
            .context("Introspection request error")?;

        if !introspection_result.active() {
            return Err(UniversalInboxError::Unauthorized(anyhow!(
                "Given access token is not active"
            )));
        }

        Ok(introspection_result
            .sub()
            .ok_or_else(|| anyhow!("No subject found in introspection result"))?
            .to_string()
            .into())
    }

    /// In an OpenIDConnect flow, the access token is fetched by the front-end and sent to the API
    /// This function validates the access token and creates the user if it does not exist.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn authenticate_for_auth_code_pkce_flow<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        openid_connect_settings: &OpenIDConnectSettings,
        pkce_flow_settings: &OIDCAuthorizationCodePKCEFlowSettings,
        access_token: AccessToken,
        id_token: IdToken<
            EmptyAdditionalClaims,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
            CoreJsonWebKeyType,
        >,
    ) -> Result<User, UniversalInboxError> {
        let mut oidc_provider = self
            .get_openid_connect_provider(openid_connect_settings)
            .await?;
        let auth_user_id: AuthUserId = self
            .verify_access_token(pkce_flow_settings, &mut oidc_provider, &access_token)
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

    /// In an OpenIDConnect flow, this function update the ID token associated with the given auth_user_id
    /// If there no user associated with the given auth_user_id, it creates a new user.
    #[tracing::instrument(level = "debug", skip_all, err)]
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
                    .parse()
                    .context("Invalid email address")?;

                self.repository
                    .create_user(
                        executor,
                        User::new(
                            first_name,
                            last_name,
                            email,
                            UserAuth::OpenIdConnect(OpenIdConnectUserAuth {
                                auth_user_id,
                                auth_id_token: id_token.to_string().into(),
                            }),
                        ),
                    )
                    .await
            }
        }
    }

    #[tracing::instrument(level = "debug", skip_all, fields(user.id = user_id.to_string()), err)]
    pub async fn close_session<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
    ) -> Result<Url, UniversalInboxError> {
        match &self.application_settings.security.authentication {
            AuthenticationSettings::OpenIDConnect(oidc_settings) => {
                match &oidc_settings.oidc_flow_settings {
                    OIDCFlowSettings::AuthorizationCodePKCEFlow { .. } => {
                        let provider_metadata = ProviderMetadataWithLogout::discover_async(
                            oidc_settings.oidc_issuer_url.clone(),
                            async_http_client,
                        )
                        .await
                        .context("metadata provider error")?;
                        let end_session_url: EndSessionUrl = provider_metadata
                            .additional_metadata()
                            .end_session_endpoint
                            .as_ref()
                            .ok_or_else(|| {
                                anyhow!("No end session endpoint found in provider metadata")
                            })?
                            .clone();
                        let logout_request: LogoutRequest = end_session_url.into();

                        let user = self
                            .repository
                            .get_user(executor, user_id)
                            .await?
                            .ok_or_else(|| anyhow!("User with ID {user_id} not found"))?;
                        let UserAuth::OpenIdConnect(user_auth) = &user.auth else {
                            return Err(anyhow!(
                                "User with ID {user_id} does not have OpenIDConnect auth parameters"
                            ))?;
                        };
                        let id_token = CoreIdToken::from_str(&user_auth.auth_id_token.to_string())
                            .context(
                                "Could not parse stored OIDC ID token, this should not happen",
                            )?;

                        Ok(logout_request
                            .set_id_token_hint(&id_token)
                            .set_post_logout_redirect_uri(PostLogoutRedirectUrl::from_url(
                                self.application_settings.front_base_url.clone(),
                            ))
                            .http_get_url())
                    }
                    OIDCFlowSettings::GoogleAuthorizationCodeFlow => Ok(format!(
                        "https://accounts.google.com/logout?continue={}",
                        self.application_settings.front_base_url.clone()
                    )
                    .parse::<Url>()
                    .unwrap()),
                }
            }
            AuthenticationSettings::Local(_) => {
                Ok(self.application_settings.front_base_url.clone())
            }
        }
    }

    pub async fn build_auth_url<'a>(
        &self,
        openid_connect_settings: &OpenIDConnectSettings,
    ) -> Result<(Url, CsrfToken, Nonce), UniversalInboxError> {
        Ok(self
            .get_openid_connect_provider(openid_connect_settings)
            .await?
            .build_google_authorization_code_flow_auth_url())
    }

    #[allow(clippy::type_complexity)]
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn fetch_access_token<'a>(
        &self,
        openid_connect_settings: &OpenIDConnectSettings,
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
            .get_openid_connect_provider(openid_connect_settings)
            .await?
            .fetch_access_token(auth_code, nonce, None)
            .await?)
    }

    async fn get_openid_connect_provider(
        &self,
        openid_connect_settings: &OpenIDConnectSettings,
    ) -> Result<OpenidConnectProvider, UniversalInboxError> {
        let redirect_url = RedirectUrl::new(
            self.application_settings
                .get_oidc_auth_code_flow_redirect_url()?
                .to_string(),
        )
        .context("Failed to build OpenID connect redirection URL from {redirect_url}")?;

        Ok(OpenidConnectProvider::build(
            openid_connect_settings.oidc_issuer_url.clone(),
            openid_connect_settings.oidc_api_client_id.clone(),
            Some(openid_connect_settings.oidc_api_client_secret.clone()),
            redirect_url,
        )
        .await?)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user.id.to_string()),
        err
    )]
    pub async fn register_user<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user: User,
    ) -> Result<User, UniversalInboxError> {
        let new_user = self.repository.create_user(executor, user).await?;
        self.send_verification_email(executor, new_user.id, false)
            .await?;
        Ok(new_user)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn validate_credentials<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        credentials: Credentials,
    ) -> Result<User, UniversalInboxError> {
        // Use a default password hash to prevent timing attacks
        let mut expected_password_hash = Secret::new(PasswordHash(
            "$argon2id$v=19$m=20000,t=2,p=1$\
                 gZiV/M1gPc22ElAH/Jh1Hw$\
                 CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
                .to_string(),
        ));

        let user = self
            .repository
            .get_user_by_email(executor, &credentials.email)
            .await?;
        if let Some(User {
            auth:
                UserAuth::Local(LocalUserAuth {
                    ref password_hash, ..
                }),
            ..
        }) = user
        {
            expected_password_hash = password_hash.clone();
        }
        spawn_blocking_with_tracing(move || {
            UserService::verify_password_hash(expected_password_hash, credentials.password)
        })
        .await
        .context("Failed to spawn blocking task.")??;

        user.ok_or_else(|| UniversalInboxError::Unauthorized(anyhow!("Unknown user")))
    }

    pub fn get_new_password_hash(
        &self,
        password: Secret<Password>,
    ) -> Result<Secret<PasswordHash>, UniversalInboxError> {
        let salt = SaltString::generate(&mut rand::thread_rng());
        let AuthenticationSettings::Local(local_auth_settings) =
            &self.application_settings.security.authentication
        else {
            return Err(anyhow!(
                "Cannot hash password without local authentication settings"
            ))?;
        };

        Ok(Argon2::new(
            local_auth_settings.argon2_algorithm,
            local_auth_settings.argon2_version,
            Params::new(
                local_auth_settings.argon2_memory_size,
                local_auth_settings.argon2_iterations,
                local_auth_settings.argon2_parallelism,
                None,
            )
            .context("Failed to build Argon2 parameters")?,
        )
        .hash_password(password.expose_secret().0.as_bytes(), &salt)
        .map(|hash| Secret::new(PasswordHash(hash.to_string())))
        .context("Failed to hash password")?)
    }

    fn verify_password_hash(
        expected_password_hash: Secret<PasswordHash>,
        password_candidate: Secret<Password>,
    ) -> Result<(), UniversalInboxError> {
        let expected_password_hash =
            argon2::PasswordHash::new(expected_password_hash.expose_secret().0.as_str())
                .context("Failed to parse hash in PHC string format.")?;

        let params: Params = (&expected_password_hash)
            .try_into()
            .context("Failed to extract Argon2 parameters from PHC string")?;
        let argon2: Argon2 = params.into();

        argon2
            .verify_password(
                password_candidate.expose_secret().0.as_bytes(),
                &expected_password_hash,
            )
            .context("Invalid password.")
            .map_err(UniversalInboxError::Unauthorized)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    pub async fn send_verification_email<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        let email_validation_token: EmailValidationToken = Uuid::new_v4().into();
        let updated_user = self
            .repository
            .update_email_validation_parameters(
                executor,
                user_id,
                None,
                Some(Utc::now()),
                Some(email_validation_token.clone()),
            )
            .await?;

        if let UpdateStatus {
            updated: true,
            result: Some(user),
        } = updated_user
        {
            let email_verification_url = format!(
                "{}users/{user_id}/email-verification/{email_validation_token}",
                self.application_settings.front_base_url
            )
            .parse()
            .context("Failed to build email validation URL")?;

            let template = EmailTemplate::EmailVerification {
                first_name: user.first_name.clone(),
                email_verification_url,
            };
            self.mailer
                .read()
                .await
                .send_email(user, template, dry_run)
                .await?;
        }

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    pub async fn verify_email<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
        email_validation_token: EmailValidationToken,
    ) -> Result<(), UniversalInboxError> {
        let stored_email_validation_token = self
            .repository
            .get_user_email_validation_token(executor, user_id)
            .await?;

        match stored_email_validation_token {
            Some(token) if token == email_validation_token => {
                self.repository
                    .update_email_validation_parameters(
                        executor,
                        user_id,
                        Some(Utc::now()),
                        None,
                        None,
                    )
                    .await?;
                Ok(())
            }
            _ => Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Invalid email validation token for user {user_id}"),
            }),
        }
    }

    #[tracing::instrument(level = "debug", skip_all, fields(email_address), err)]
    pub async fn send_password_reset_email<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        email_address: EmailAddress,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        let password_reset_token: PasswordResetToken = Uuid::new_v4().into();
        let updated_user = self
            .repository
            .update_password_reset_parameters(
                executor,
                email_address.clone(),
                Some(Utc::now()),
                Some(password_reset_token.clone()),
            )
            .await?;

        match updated_user {
            UpdateStatus {
                updated: true,
                result: Some(user),
            } => {
                let password_reset_url = format!(
                    "{}users/{}/password-reset/{password_reset_token}",
                    self.application_settings.front_base_url, user.id,
                )
                .parse()
                .context("Failed to build reset password URL")?;

                let template = EmailTemplate::PasswordReset {
                    first_name: user.first_name.clone(),
                    password_reset_url,
                };
                self.mailer
                    .read()
                    .await
                    .send_email(user, template, dry_run)
                    .await?;
            }
            UpdateStatus {
                updated: false,
                result: None,
            } => {
                warn!("No user found for email address {email_address}");
            }
            _ => {
                error!("User not updated while resetting password for email address {email_address}, should not happen");
            }
        }

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    pub async fn reset_password<'a>(
        &self,
        executor: &mut Transaction<'a, Postgres>,
        user_id: UserId,
        password_reset_token: PasswordResetToken,
        new_password: Secret<Password>,
    ) -> Result<(), UniversalInboxError> {
        let new_password_hash = self.get_new_password_hash(new_password)?;
        let updated_user = self
            .repository
            .update_password(
                executor,
                user_id,
                new_password_hash,
                Some(password_reset_token.clone()),
            )
            .await?;

        match updated_user {
            UpdateStatus {
                result: Some(_), ..
            } => Ok(()),
            UpdateStatus { result: None, .. } => Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Invalid password reset token for user {user_id}"),
            }),
        }
    }
}
