use std::{str::FromStr, sync::Arc};

use anyhow::{anyhow, Context};
use argon2::{password_hash::SaltString, Argon2, Params, PasswordHasher, PasswordVerifier};
use chrono::Utc;
use email_address::EmailAddress;
use openidconnect::{
    core::{
        CoreGenderClaim, CoreIdToken, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
        CoreUserInfoClaims,
    },
    AccessToken, AuthorizationCode, CsrfToken, EmptyAdditionalClaims, EndSessionUrl, IdToken,
    LogoutRequest, Nonce, PostLogoutRedirectUrl, ProviderMetadataWithLogout, RedirectUrl,
    SubjectIdentifier, TokenIntrospectionResponse,
};
use secrecy::{ExposeSecret, SecretBox};
use sqlx::{Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use universal_inbox::{
    auth::openidconnect::OpenidConnectProvider,
    user::{
        Credentials, EmailValidationToken, Password, PasswordHash, PasswordResetToken, User,
        UserId, Username,
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
    universal_inbox::{
        user::model::{AuthUserId, OpenIdConnectUserAuth, PasskeyUserAuth, UserAuth, UserAuthKind},
        UniversalInboxError, UpdateStatus,
    },
};

pub struct UserService {
    repository: Arc<Repository>,
    application_settings: ApplicationSettings,
    mailer: Arc<RwLock<dyn Mailer + Send + Sync>>,
    webauthn: Arc<Webauthn>,
}

impl UserService {
    pub fn new(
        repository: Arc<Repository>,
        application_settings: ApplicationSettings,
        mailer: Arc<RwLock<dyn Mailer + Send + Sync>>,
        webauthn: Arc<Webauthn>,
    ) -> UserService {
        UserService {
            repository,
            application_settings,
            mailer,
            webauthn,
        }
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, UniversalInboxError> {
        self.repository.begin().await
    }

    pub async fn get_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        id: UserId,
    ) -> Result<Option<User>, UniversalInboxError> {
        let user_result = self.repository.get_user(executor, id).await?;
        if let Some(
            user @ User {
                email: Some(email),
                email_validated_at: Some(_),
                ..
            },
        ) = &user_result
        {
            if let Some(chat_support_settings) = &self.application_settings.chat_support {
                let chat_support_email_signature =
                    Some(chat_support_settings.sign_email(email.as_str()));
                return Ok(Some(User {
                    chat_support_email_signature,
                    ..user.clone()
                }));
            }
        }

        Ok(user_result)
    }

    pub async fn get_user_by_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email: &EmailAddress,
    ) -> Result<Option<User>, UniversalInboxError> {
        self.repository.get_user_by_email(executor, email).await
    }

    pub async fn fetch_all_users(
        &self,
        executor: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<User>, UniversalInboxError> {
        self.repository.fetch_all_users(executor).await
    }

    pub async fn fetch_all_users_and_auth(
        &self,
        executor: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<(User, UserAuth)>, UniversalInboxError> {
        self.repository.fetch_all_users_and_auth(executor).await
    }

    pub async fn delete_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<bool, UniversalInboxError> {
        self.repository.delete_user(executor, user_id).await
    }

    /// In an OpenID Connect Authorization code flow, the API has fetched the access token and
    /// thus does not need to validate it.
    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn authenticate_for_auth_code_flow(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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
            UserAuth::OIDCGoogleAuthorizationCode(Box::new(OpenIdConnectUserAuth {
                auth_user_id,
                auth_id_token: id_token.to_string().into(),
            })),
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
            .set_introspection_url(pkce_flow_settings.introspection_url.clone());

        let introspection_result = client
            .introspect(access_token)
            .set_token_type_hint("access_token")
            .request_async(&oidc_provider.http_client)
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
    pub async fn authenticate_for_auth_code_pkce_flow(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        openid_connect_settings: &OpenIDConnectSettings,
        pkce_flow_settings: &OIDCAuthorizationCodePKCEFlowSettings,
        access_token: AccessToken,
        id_token: IdToken<
            EmptyAdditionalClaims,
            CoreGenderClaim,
            CoreJweContentEncryptionAlgorithm,
            CoreJwsSigningAlgorithm,
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
            UserAuth::OIDCAuthorizationCodePKCE(Box::new(OpenIdConnectUserAuth {
                auth_user_id,
                auth_id_token: id_token.to_string().into(),
            })),
        )
        .await
    }

    /// In an OpenIDConnect flow, this function update the ID token associated with the given auth_user_id
    /// If there no user associated with the given auth_user_id, it creates a new user.
    #[tracing::instrument(level = "debug", skip_all, err)]
    async fn authenticate_and_create_user_if_not_exists(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        oidc_provider: OpenidConnectProvider,
        // the access token must have been validated before calling this function
        access_token: AccessToken,
        user_auth: UserAuth,
    ) -> Result<User, UniversalInboxError> {
        let oidc_user_auth = match &user_auth {
            UserAuth::OIDCAuthorizationCodePKCE(oidc_user_auth) => oidc_user_auth,
            UserAuth::OIDCGoogleAuthorizationCode(oidc_user_auth) => oidc_user_auth,
            _ => {
                return Err(anyhow!(
                    "Expected OpenIDConnect UserAuth, got {:?}",
                    user_auth
                ))?
            }
        };

        match self
            .repository
            .update_user_auth_id_token(
                executor,
                &oidc_user_auth.auth_user_id,
                &oidc_user_auth.auth_id_token.to_string().into(),
            )
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
                info!(
                    "User with auth provider user ID {} does not exists, creating a new one",
                    oidc_user_auth.auth_user_id
                );
                let user_infos: CoreUserInfoClaims = oidc_provider
                    .client
                    .user_info(
                        access_token,
                        Some(SubjectIdentifier::new(
                            oidc_user_auth.auth_user_id.to_string(),
                        )),
                    )
                    .context("UserInfo configuration error")?
                    .request_async(&oidc_provider.http_client)
                    .await
                    .context("UserInfo request error")?;

                let first_name = user_infos
                    .given_name()
                    .and_then(|name| name.get(None))
                    .map(|name| name.to_string());
                let last_name = user_infos
                    .family_name()
                    .and_then(|name| name.get(None))
                    .map(|name| name.to_string());
                let email: EmailAddress = user_infos
                    .email()
                    .context("No email found in user info")?
                    .parse()
                    .context("Invalid email address")?;

                // Check if the email domain is blacklisted
                let domain = email.domain().to_lowercase();
                if let Some(rejection_message) = self
                    .application_settings
                    .security
                    .email_domain_blacklist
                    .get(&domain)
                {
                    return Err(UniversalInboxError::Forbidden(rejection_message.clone()));
                }

                self.repository
                    .create_user(executor, User::new(first_name, last_name, email), user_auth)
                    .await
            }
        }
    }

    #[tracing::instrument(level = "debug", skip_all, fields(user.id = user_id.to_string()), err)]
    pub async fn close_session(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        user_auth_kind: Option<UserAuthKind>,
    ) -> Result<Url, UniversalInboxError> {
        let Some(user_auth_kind) = user_auth_kind else {
            return Ok(self.application_settings.front_base_url.clone());
        };
        let auth_settings = self
            .application_settings
            .security
            .get_authentication_settings(user_auth_kind)
            .ok_or_else(|| {
                anyhow!(
                    "Unable to find configuration for {} authentication settings",
                    user_auth_kind
                )
            })?;

        match &auth_settings {
            AuthenticationSettings::OpenIDConnect(oidc_settings) => {
                match &oidc_settings.oidc_flow_settings {
                    OIDCFlowSettings::AuthorizationCodePKCEFlow { .. } => {
                        let oidc_provider = self.get_openid_connect_provider(oidc_settings).await?;
                        let provider_metadata = ProviderMetadataWithLogout::discover_async(
                            oidc_settings.oidc_issuer_url.clone(),
                            &oidc_provider.http_client,
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

                        let user_auth = self
                            .repository
                            .get_user_auth(executor, user_id)
                            .await?
                            .ok_or_else(|| anyhow!("User with ID {user_id} not found"))?;
                        let UserAuth::OIDCAuthorizationCodePKCE(user_auth) = &user_auth else {
                            return Err(anyhow!(
                                "User with ID {user_id} does not have OIDCAuthorizationCodePKCE auth parameters"
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
                    OIDCFlowSettings::GoogleAuthorizationCodeFlow => {
                        Ok(self.application_settings.front_base_url.clone())
                    }
                }
            }
            AuthenticationSettings::Local(_) | AuthenticationSettings::Passkey => {
                Ok(self.application_settings.front_base_url.clone())
            }
        }
    }

    pub async fn build_auth_url(
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
    pub async fn fetch_access_token(
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
    pub async fn register_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user: User,
        user_auth: UserAuth,
    ) -> Result<User, UniversalInboxError> {
        let new_user = self
            .repository
            .create_user(executor, user, user_auth)
            .await?;
        self.send_verification_email(executor, new_user.id, false)
            .await?;
        Ok(new_user)
    }

    #[tracing::instrument(level = "debug", skip_all, err)]
    pub async fn validate_credentials(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        credentials: Credentials,
    ) -> Result<User, UniversalInboxError> {
        // Use a default password hash to prevent timing attacks
        let mut expected_password_hash = SecretBox::new(Box::new(PasswordHash(
            "$argon2id$v=19$m=20000,t=2,p=1$\
                 gZiV/M1gPc22ElAH/Jh1Hw$\
                 CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
                .to_string(),
        )));
        let mut result_user_id = None;

        if let Some((UserAuth::Local(local_user_auth), user_id)) = self
            .repository
            .get_user_auth_by_email(executor, &credentials.email)
            .await?
        {
            expected_password_hash = local_user_auth.password_hash;
            result_user_id = Some(user_id);
        }
        spawn_blocking_with_tracing(move || {
            UserService::verify_password_hash(expected_password_hash, credentials.password)
        })
        .await
        .context("Failed to spawn blocking task.")??;

        let user_id = result_user_id
            .ok_or_else(|| UniversalInboxError::Unauthorized(anyhow!("Unknown user")))?;
        self.repository
            .get_user(executor, user_id)
            .await?
            .ok_or_else(|| UniversalInboxError::Unauthorized(anyhow!("Unknown user")))
    }

    pub fn get_new_password_hash(
        &self,
        password: SecretBox<Password>,
    ) -> Result<SecretBox<PasswordHash>, UniversalInboxError> {
        let salt = SaltString::generate(&mut rand::thread_rng());
        let Some(AuthenticationSettings::Local(local_auth_settings)) = &self
            .application_settings
            .security
            .authentication
            .iter()
            .find(|auth_settings| matches!(auth_settings, AuthenticationSettings::Local(_)))
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
        .map(|hash| SecretBox::new(Box::new(PasswordHash(hash.to_string()))))
        .context("Failed to hash password")?)
    }

    fn verify_password_hash(
        expected_password_hash: SecretBox<PasswordHash>,
        password_candidate: SecretBox<Password>,
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
    pub async fn send_verification_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        // Skip sending verification email for test accounts
        let user = self.repository.get_user(executor, user_id).await?;
        if let Some(user) = user {
            if user.is_testing {
                debug!("Skipping verification email for test account {user_id}");
                return Ok(());
            }
        }

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
    pub async fn verify_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
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
    pub async fn send_password_reset_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email_address: EmailAddress,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        // Skip sending password reset email for test accounts
        let user = self
            .repository
            .get_user_by_email(executor, &email_address)
            .await?;
        if let Some(user) = user {
            if user.is_testing {
                debug!("Skipping password reset email for test account {email_address}");
                return Ok(());
            }
        }

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
    pub async fn reset_password(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        password_reset_token: PasswordResetToken,
        new_password: SecretBox<Password>,
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(username = username.to_string()),
        err
    )]
    pub async fn start_passkey_registration(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        username: &Username,
    ) -> Result<(UserId, CreationChallengeResponse, PasskeyRegistration), UniversalInboxError> {
        if let Some((_, user_id)) = self
            .repository
            .get_user_auth_by_username(executor, username)
            .await?
        {
            return Err(UniversalInboxError::AlreadyExists {
                source: None,
                id: user_id.0,
            });
        }
        let user_id: UserId = Uuid::new_v4().into();
        let (creation_challenge_response, passkey_registration) = self
            .webauthn
            .start_passkey_registration(user_id.0, username.0.as_str(), username.0.as_str(), None)
            .with_context(|| format!("Failed to start Passkey registration for {username}"))?;
        Ok((user_id, creation_challenge_response, passkey_registration))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            username = username.to_string(),
            user.id = user_id.to_string(),
        ),
        err
    )]
    pub async fn finish_passkey_registration(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        username: &Username,
        user_id: UserId,
        register_credentials: RegisterPublicKeyCredential,
        passkey_registration: PasskeyRegistration,
    ) -> Result<User, UniversalInboxError> {
        let passkey = self
            .webauthn
            .finish_passkey_registration(&register_credentials, &passkey_registration)
            .with_context(|| format!("Failed to finish Passkey registration for {username}"))?;

        let user = User::new_with_passkey(user_id);
        let user_auth = UserAuth::Passkey(Box::new(PasskeyUserAuth {
            username: username.clone(),
            passkey: passkey.clone(),
        }));

        let new_user = self
            .repository
            .create_user(executor, user, user_auth)
            .await?;

        Ok(new_user)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(username = username.to_string()),
        err
    )]
    pub async fn start_passkey_authentication(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        username: &Username,
    ) -> Result<(UserId, RequestChallengeResponse, PasskeyAuthentication), UniversalInboxError>
    {
        let Some((user_auth, user_id)) = self
            .repository
            .get_user_auth_by_username(executor, username)
            .await?
        else {
            return Err(UniversalInboxError::ItemNotFound(format!(
                "No user found for username {username}"
            )));
        };
        let UserAuth::Passkey(passkey_user_auth) = user_auth else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "No passkey found for user with username {username}"
            )));
        };

        let (request_challenge_response, passkey_authentication) = self
            .webauthn
            .start_passkey_authentication(&[passkey_user_auth.passkey])
            .with_context(|| {
                format!("Failed to start Passkey authentication for user with username {username}")
            })?;

        Ok((user_id, request_challenge_response, passkey_authentication))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(user.id = user_id.to_string()),
        err
    )]
    pub async fn finish_passkey_authentication(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        credentials: PublicKeyCredential,
        passkey_authentication: PasskeyAuthentication,
    ) -> Result<User, UniversalInboxError> {
        let auth_result = self
            .webauthn
            .finish_passkey_authentication(&credentials, &passkey_authentication)
            .with_context(|| {
                format!("Failed to finish Passkey authentication for user {user_id}")
            })?;

        let Some(user_auth) = self.repository.get_user_auth(executor, user_id).await? else {
            return Err(UniversalInboxError::ItemNotFound(format!(
                "No user {user_id} found"
            )));
        };
        let UserAuth::Passkey(mut passkey_user_auth) = user_auth else {
            return Err(UniversalInboxError::Unexpected(anyhow!(
                "No passkey found for user {user_id}"
            )));
        };
        let Some(user) = self.repository.get_user(executor, user_id).await? else {
            return Err(UniversalInboxError::ItemNotFound(format!(
                "No user {user_id} found"
            )));
        };

        if passkey_user_auth
            .passkey
            .update_credential(&auth_result)
            .unwrap_or_default()
        {
            self.repository
                .update_passkey(executor, &user_id, &passkey_user_auth.passkey)
                .await?;
        }

        Ok(user)
    }
}
