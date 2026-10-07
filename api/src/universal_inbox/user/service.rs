use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
};

use anyhow::{Context, anyhow};
use argon2::{Argon2, Params, PasswordHasher, PasswordVerifier};
use chrono::{DateTime, TimeDelta, Utc};
use email_address::EmailAddress;
use futures::{FutureExt, future::LocalBoxFuture};
use openidconnect::{
    AccessToken, AuthorizationCode, CsrfToken, EmptyAdditionalClaims, EndSessionUrl, IdToken,
    LogoutRequest, Nonce, PostLogoutRedirectUrl, ProviderMetadataWithLogout, RedirectUrl,
    SubjectIdentifier, TokenIntrospectionResponse,
    core::{
        CoreGenderClaim, CoreIdToken, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
        CoreUserInfoClaims,
    },
};
use secrecy::{ExposeSecret, SecretBox};
use sqlx::{Acquire, Postgres, Transaction};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use universal_inbox::pii::Pii;
use url::Url;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use universal_inbox::{
    auth::openidconnect::OpenidConnectProvider,
    user::{
        Credentials, DeleteAccountParameters, EmailValidationToken, Password, PasswordChange,
        PasswordHash, PasswordResetToken, User, UserAuthKind, UserAuthMethod, UserId, UserPatch,
        UserPreferences, UserPreferencesPatch, Username,
    },
};

use crate::observability::attr;
use crate::{
    billing::repository::BillingRepository,
    billing::service::BillingService,
    configuration::{
        ApplicationSettings, AuthenticationSettings, OIDCAuthorizationCodePKCEFlowSettings,
        OIDCFlowSettings, OpenIDConnectSettings,
    },
    mailer::{EmailTemplate, Mailer},
    middlewares::jwt_auth::SessionTokenChecker,
    observability::spawn_blocking_with_tracing,
    repository::Repository,
    repository::auth_token::AuthenticationTokenRepository,
    repository::integration_connection::IntegrationConnectionRepository,
    repository::notification::NotificationRepository,
    repository::oauth2::OAuth2Repository,
    repository::task::TaskRepository,
    repository::user::UserRepository,
    repository::user_preferences::UserPreferencesRepository,
    universal_inbox::integration_connection::service::IntegrationConnectionService,
    universal_inbox::{
        UniversalInboxError, UpdateStatus,
        user::model::{
            AuthUserId, LocalUserAuth, OpenIdConnectUserAuth, PasskeyUserAuth, UserAuth,
            UserDataExport,
        },
    },
    utils::{
        login_throttle::{AccountRateLimitScope, LoginThrottle},
        session_revocation::SessionRevocation,
    },
};

/// How long a password reset link stays valid after its email was sent. An
/// expired link is rejected like an unknown one.
const PASSWORD_RESET_VALIDITY_MINUTES: i64 = 60;

/// How long the verification link of a pending email change stays valid.
const EMAIL_CHANGE_VALIDITY_HOURS: i64 = 24;

/// Minimum delay between two writes of a user's `last_active_at`. The
/// inactivity policy counts in days, so finer precision would only cost
/// writes.
const USER_ACTIVITY_RECORDING_INTERVAL_HOURS: i64 = 24;

/// Rejects session cookies issued before the user's sessions were revoked (see
/// [`UserService::is_session_active`]).
pub struct UserSessionChecker(pub Arc<UserService>);

impl SessionTokenChecker for UserSessionChecker {
    fn is_active(
        &self,
        subject: String,
        token_id: String,
        issued_at: i64,
    ) -> LocalBoxFuture<'static, Result<bool, UniversalInboxError>> {
        let service = self.0.clone();
        async move {
            let user_id = subject
                .parse::<UserId>()
                .context("Wrong user ID format")
                .map_err(UniversalInboxError::Unexpected)?;
            service
                .is_session_active(user_id, &token_id, issued_at)
                .await
        }
        .boxed_local()
    }
}

pub struct UserService {
    repository: Arc<Repository>,
    application_settings: ApplicationSettings,
    mailer: Arc<RwLock<dyn Mailer + Send + Sync>>,
    webauthn: Arc<Webauthn>,
    /// Per-account login throttle. `None` when local password auth is not
    /// configured (nothing to throttle).
    login_throttle: Option<LoginThrottle>,
    /// Revokes a session on logout, and all of a user's sessions on password
    /// change or reset.
    session_revocation: SessionRevocation,
    /// Used on account deletion to revoke the user's provider OAuth grants.
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    /// When this process last recorded each user's activity, so that most
    /// authenticated requests skip the database entirely.
    recorded_user_activities: Mutex<HashMap<UserId, DateTime<Utc>>>,
}

impl UserService {
    pub fn new(
        repository: Arc<Repository>,
        application_settings: ApplicationSettings,
        mailer: Arc<RwLock<dyn Mailer + Send + Sync>>,
        webauthn: Arc<Webauthn>,
        login_throttle: Option<LoginThrottle>,
        session_revocation: SessionRevocation,
        integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    ) -> UserService {
        UserService {
            repository,
            application_settings,
            mailer,
            webauthn,
            login_throttle,
            session_revocation,
            integration_connection_service,
            recorded_user_activities: Mutex::new(HashMap::new()),
        }
    }

    /// Record that the user just used Universal Inbox, for the inactivity
    /// policy pausing the integrations of long-gone users. Writes
    /// `last_active_at` at most once per
    /// [`USER_ACTIVITY_RECORDING_INTERVAL_HOURS`]; returns whether it wrote.
    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
    pub async fn record_user_activity(&self, user_id: UserId) -> Result<bool, UniversalInboxError> {
        let now = Utc::now();
        let not_before = now - TimeDelta::hours(USER_ACTIVITY_RECORDING_INTERVAL_HOURS);
        let recorded_recently = self
            .recorded_user_activities
            .lock()
            .map_err(|_| anyhow!("User activities cache lock is poisoned"))?
            .get(&user_id)
            .is_some_and(|recorded_at| *recorded_at > not_before);
        if recorded_recently {
            return Ok(false);
        }

        let mut transaction = self
            .begin()
            .await
            .context("Failed to create new transaction while recording user activity")?;
        let written = self
            .repository
            .touch_user_last_active_at(&mut transaction, user_id, now, not_before)
            .await?;
        transaction
            .commit()
            .await
            .context("Failed to commit while recording user activity")?;

        let mut recorded_user_activities = self
            .recorded_user_activities
            .lock()
            .map_err(|_| anyhow!("User activities cache lock is poisoned"))?;
        // Drop stale entries so the memo only holds recently active users.
        recorded_user_activities.retain(|_, recorded_at| *recorded_at > not_before);
        recorded_user_activities.insert(user_id, now);
        Ok(written)
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
            && let Some(chat_support_settings) = &self.application_settings.chat_support
        {
            let chat_support_email_signature =
                Some(chat_support_settings.sign_email(email.expose().as_str()));
            return Ok(Some(User {
                chat_support_email_signature,
                ..user.clone()
            }));
        }

        Ok(user_result)
    }

    /// Gathers all the data stored for `user_id` for the user data export.
    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
    pub async fn export_user_data(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<UserDataExport, UniversalInboxError> {
        // Read from the repository rather than `get_user` so the export does
        // not carry the server-computed chat support signature.
        let user = self
            .repository
            .get_user(executor, user_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::ItemNotFound(format!("User {user_id} not found"))
            })?;

        Ok(UserDataExport {
            exported_at: Utc::now(),
            user,
            preferences: self.get_user_preferences(executor, user_id).await?,
            auth_methods: self.list_user_auth_methods(executor, user_id).await?,
            authentication_tokens: self
                .repository
                .fetch_auth_tokens_for_user(executor, user_id, true)
                .await?,
            oauth2_authorized_clients: self
                .repository
                .list_authorized_clients(executor, user_id)
                .await?,
            integration_connections: self
                .repository
                .fetch_all_integration_connections(executor, user_id, None, false)
                .await?,
            subscription: self
                .repository
                .get_user_subscription(executor, user_id)
                .await?,
            notifications: self
                .repository
                .fetch_all_notifications_for_user(executor, user_id)
                .await?,
            tasks: self
                .repository
                .fetch_all_tasks_for_user(executor, user_id)
                .await?,
        })
    }

    pub async fn get_user_by_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email: &Pii<EmailAddress>,
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
    ) -> Result<Vec<(User, Vec<UserAuth>)>, UniversalInboxError> {
        self.repository.fetch_all_users_and_auth(executor).await
    }

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
    pub async fn list_user_auth_methods(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Vec<UserAuthMethod>, UniversalInboxError> {
        let user_auths = self
            .repository
            .get_all_user_auths(executor, user_id, false)
            .await?;
        Ok(user_auths.iter().map(UserAuthMethod::from).collect())
    }

    pub async fn get_user_auth(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        kind: UserAuthKind,
    ) -> Result<Option<UserAuth>, UniversalInboxError> {
        self.repository.get_user_auth(executor, user_id, kind).await
    }

    // --- Auth method management (add/remove) ---

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
    pub async fn add_local_auth_method(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        password: SecretBox<Password>,
    ) -> Result<UserAuthMethod, UniversalInboxError> {
        if self
            .repository
            .get_user_auth(executor, user_id, UserAuthKind::Local)
            .await?
            .is_some()
        {
            return Err(UniversalInboxError::AlreadyExists {
                source: None,
                id: user_id.0,
            });
        }

        let password_hash = self.get_new_password_hash(password)?;
        let user_auth = UserAuth::Local(Box::new(LocalUserAuth {
            password_hash,
            password_reset_at: None,
            password_reset_sent_at: None,
        }));

        let auth_method = UserAuthMethod::from(&user_auth);
        self.repository
            .create_user_auth(executor, user_id, user_auth)
            .await?;

        Ok(auth_method)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn start_add_passkey_auth_method(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        username: &Username,
    ) -> Result<(CreationChallengeResponse, PasskeyRegistration), UniversalInboxError> {
        if self
            .repository
            .get_user_auth(executor, user_id, UserAuthKind::Passkey)
            .await?
            .is_some()
        {
            return Err(UniversalInboxError::AlreadyExists {
                source: None,
                id: user_id.0,
            });
        }

        // Check that username is not already taken by another user
        if let Some((_, existing_user_id)) = self
            .repository
            .get_user_auth_by_username(executor, username)
            .await?
            && existing_user_id != user_id
        {
            return Err(UniversalInboxError::AlreadyExists {
                source: None,
                id: existing_user_id.0,
            });
        }

        let (creation_challenge_response, passkey_registration) = self
            .webauthn
            .start_passkey_registration(user_id.0, username.0.as_str(), username.0.as_str(), None)
            .with_context(|| format!("Failed to start Passkey registration for user {user_id}"))?;

        Ok((creation_challenge_response, passkey_registration))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn finish_add_passkey_auth_method(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        username: &Username,
        user_id: UserId,
        register_credentials: RegisterPublicKeyCredential,
        passkey_registration: PasskeyRegistration,
    ) -> Result<UserAuthMethod, UniversalInboxError> {
        let passkey = self
            .webauthn
            .finish_passkey_registration(&register_credentials, &passkey_registration)
            .context("Failed to finish Passkey registration")?;

        let user_auth = UserAuth::Passkey(Box::new(PasskeyUserAuth {
            username: username.clone(),
            passkey,
        }));

        let auth_method = UserAuthMethod::from(&user_auth);
        self.repository
            .create_user_auth(executor, user_id, user_auth)
            .await?;

        Ok(auth_method)
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string(), { attr::USER_AUTH_KIND } = kind.to_string())
    )]
    pub async fn remove_auth_method(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        kind: UserAuthKind,
    ) -> Result<(), UniversalInboxError> {
        // Lock the user's auth rows to prevent concurrent removals from
        // deleting the last authentication method (TOCTOU).
        let auth_methods = self
            .repository
            .get_all_user_auths(executor, user_id, true)
            .await?;

        if auth_methods.len() <= 1 {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "Cannot remove the last authentication method".to_string(),
            });
        }

        if !auth_methods.iter().any(|auth| auth.kind() == kind) {
            return Err(UniversalInboxError::ItemNotFound(format!(
                "No {kind} authentication method found for user {user_id}"
            )));
        }

        self.repository
            .delete_user_auth(executor, user_id, kind)
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
    pub async fn link_oidc_auth_method(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        user_auth: UserAuth,
        oidc_email: Pii<EmailAddress>,
    ) -> Result<UserAuthMethod, UniversalInboxError> {
        let kind = user_auth.kind();

        if !matches!(
            &user_auth,
            UserAuth::OIDCAuthorizationCodePKCE(_) | UserAuth::OIDCGoogleAuthorizationCode(_)
        ) {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Cannot link non-OIDC authentication method {kind}"),
            });
        }

        if self
            .repository
            .get_user_auth(executor, user_id, kind)
            .await?
            .is_some()
        {
            return Err(UniversalInboxError::AlreadyExists {
                source: None,
                id: user_id.0,
            });
        }

        // Validate OIDC email against user's current email
        let user = self
            .repository
            .get_user(executor, user_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::ItemNotFound(format!("User {user_id} not found"))
            })?;

        match &user.email {
            Some(current_email) if *current_email != oidc_email => {
                return Err(UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: format!(
                        "The email from the OIDC account ({}) does not match your current email ({})",
                        oidc_email.expose(),
                        current_email.expose()
                    ),
                });
            }
            None => {
                // User has no email yet, set it from the OIDC provider
                // Check if the email domain is blacklisted
                let domain = oidc_email.expose().domain().to_lowercase();
                if let Some(rejection_message) = self
                    .application_settings
                    .security
                    .email_domain_blacklist
                    .get(&domain)
                {
                    return Err(UniversalInboxError::Forbidden(rejection_message.clone()));
                }
                self.repository
                    .update_user_profile(
                        executor,
                        user_id,
                        &UserPatch {
                            email: Some(oidc_email),
                            ..Default::default()
                        },
                    )
                    .await?;
            }
            _ => {} // Emails match, proceed
        }

        let auth_method = UserAuthMethod::from(&user_auth);
        self.repository
            .create_user_auth(executor, user_id, user_auth)
            .await?;

        Ok(auth_method)
    }

    /// Link an OIDC auth method to an existing user via the Authorization Code flow (Google).
    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
    pub async fn link_for_auth_code_flow(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        openid_connect_settings: &OpenIDConnectSettings,
        code: AuthorizationCode,
        nonce: Nonce,
    ) -> Result<UserAuthMethod, UniversalInboxError> {
        let (access_token, id_token) = self
            .fetch_access_token(openid_connect_settings, code, nonce.clone())
            .await?;

        let oidc_provider = self
            .get_openid_connect_provider(openid_connect_settings)
            .await?;
        let auth_user_id: AuthUserId = oidc_provider
            .verify_id_token_claims(&id_token, &nonce)?
            .subject()
            .to_string()
            .into();

        let oidc_email = self
            .fetch_oidc_user_email(&oidc_provider, access_token, &auth_user_id)
            .await?;

        let user_auth = UserAuth::OIDCGoogleAuthorizationCode(Box::new(OpenIdConnectUserAuth {
            auth_user_id,
            auth_id_token: id_token.to_string().into(),
        }));

        self.link_oidc_auth_method(executor, user_id, user_auth, oidc_email)
            .await
    }

    /// Link an OIDC auth method to an existing user via the PKCE flow.
    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
    pub async fn link_for_auth_code_pkce_flow(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        openid_connect_settings: &OpenIDConnectSettings,
        pkce_flow_settings: &OIDCAuthorizationCodePKCEFlowSettings,
        access_token: AccessToken,
        id_token: CoreIdToken,
    ) -> Result<UserAuthMethod, UniversalInboxError> {
        let mut oidc_provider = self
            .get_openid_connect_provider(openid_connect_settings)
            .await?;
        let auth_user_id: AuthUserId = self
            .verify_access_token(pkce_flow_settings, &mut oidc_provider, &access_token)
            .await?;

        let oidc_email = self
            .fetch_oidc_user_email(&oidc_provider, access_token, &auth_user_id)
            .await?;

        let user_auth = UserAuth::OIDCAuthorizationCodePKCE(Box::new(OpenIdConnectUserAuth {
            auth_user_id,
            auth_id_token: id_token.to_string().into(),
        }));

        self.link_oidc_auth_method(executor, user_id, user_auth, oidc_email)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    /// Self-service account deletion: check the user's confirmation, then run
    /// the same deletion flow as the `user delete` CLI command
    /// ([`Self::delete_user`]).
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn delete_account(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        params: &DeleteAccountParameters,
        billing_service: Option<&BillingService>,
    ) -> Result<(), UniversalInboxError> {
        let user = self
            .repository
            .get_user(executor, user_id)
            .await?
            .ok_or_else(|| {
                UniversalInboxError::ItemNotFound(format!("Cannot find user {user_id}"))
            })?;

        if !user.is_account_deletion_confirmed(&params.confirmation) {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!(
                    "To confirm the deletion of your account, type {}",
                    user.account_deletion_confirmation()
                ),
            });
        }

        if !self.delete_user(executor, user_id, billing_service).await? {
            return Err(UniversalInboxError::ItemNotFound(format!(
                "Cannot find user {user_id}"
            )));
        }

        info!("User {user_id} deleted their account");
        Ok(())
    }

    /// Delete a user account and all its data (the `user` row cascades to every
    /// table owned by the user).
    ///
    /// When billing is enabled (`billing_service` is `Some`), the user's Stripe
    /// subscription is cancelled first; a Stripe failure aborts the deletion
    /// so a deleted user can never keep being charged. With billing disabled
    /// (self-hosted default) no Stripe code runs. The OAuth grants of the
    /// user's integration connections are then revoked at the providers
    /// (best effort).
    pub async fn delete_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        billing_service: Option<&BillingService>,
    ) -> Result<bool, UniversalInboxError> {
        if let Some(billing_service) = billing_service {
            billing_service
                .cancel_billing_for_account_deletion(executor, user_id)
                .await?;
        }

        // Best effort (never blocks the deletion): drop the OAuth grants at
        // the providers so they stop listing Universal Inbox as authorized.
        self.integration_connection_service
            .read()
            .await
            .revoke_all_provider_grants(executor, user_id)
            .await?;

        self.repository.delete_user(executor, user_id).await
    }

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
    pub async fn patch_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        patch: &UserPatch,
    ) -> Result<UpdateStatus<User>, UniversalInboxError> {
        // Block email changes when an OIDC authentication method is linked
        if patch.email.is_some() {
            let has_oidc = self
                .repository
                .get_all_user_auths(executor, user_id, false)
                .await?
                .iter()
                .any(|auth| {
                    matches!(
                        auth.kind(),
                        UserAuthKind::OIDCGoogleAuthorizationCode
                            | UserAuthKind::OIDCAuthorizationCodePKCE
                    )
                });

            if has_oidc {
                return Err(UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error:
                        "Email cannot be changed while an OIDC authentication method is linked"
                            .to_string(),
                });
            }
        }

        // Check email domain blacklist if email is being changed
        if let Some(email) = &patch.email {
            let domain = email.expose().domain().to_lowercase();
            if let Some(rejection_message) = self
                .application_settings
                .security
                .email_domain_blacklist
                .get(&domain)
            {
                return Err(UniversalInboxError::Forbidden(rejection_message.clone()));
            }
        }

        let Some(current_user) = self.repository.get_user(executor, user_id).await? else {
            return Ok(UpdateStatus {
                updated: false,
                result: None,
            });
        };

        // An email change is never written straight to the user: it is held
        // as a pending change and applied only once the new address is
        // verified (see `verify_email`). Writing it immediately surfaced the
        // unique-email violation to the caller, which made this endpoint an
        // oracle for "is this address registered?". The response is now the
        // same whether or not the address is taken.
        let requested_email = patch
            .email
            .as_ref()
            .filter(|email| current_user.email.as_ref() != Some(*email));
        let mut current_user = current_user;
        if let Some(new_email) = requested_email
            && self
                .request_email_change(executor, &current_user, new_email)
                .await?
            && let Some(updated_user) = self.repository.get_user(executor, user_id).await?
        {
            current_user = updated_user;
        }

        if patch.first_name.is_none() && patch.last_name.is_none() {
            return Ok(match (requested_email, &patch.email) {
                (Some(_), _) => UpdateStatus {
                    updated: true,
                    result: Some(current_user),
                },
                (None, Some(_)) => UpdateStatus {
                    updated: false,
                    result: Some(current_user),
                },
                (None, None) => UpdateStatus {
                    updated: false,
                    result: None,
                },
            });
        }

        let profile_patch = UserPatch {
            first_name: patch.first_name.clone(),
            last_name: patch.last_name.clone(),
            email: None,
        };
        let mut update_status = self
            .repository
            .update_user_profile(executor, user_id, &profile_patch)
            .await?;
        if requested_email.is_some() {
            update_status.updated = true;
        }
        Ok(update_status)
    }

    /// Record `new_email` as the user's pending email address and send the
    /// verification link to it. Replaces any previous pending change.
    ///
    /// When email is disabled, no link can be sent: the change is applied
    /// immediately instead, and `Ok(true)` is returned. A taken address is
    /// then rejected, which reveals it is registered: an accepted trade-off
    /// for instances without email.
    async fn request_email_change(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user: &User,
        new_email: &Pii<EmailAddress>,
    ) -> Result<bool, UniversalInboxError> {
        if !self.is_email_enabled().await {
            if !self
                .repository
                .apply_verified_email_change(executor, user.id, new_email)
                .await?
            {
                return Err(UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "This email address cannot be used for this account".to_string(),
                });
            }
            return Ok(true);
        }

        let validation_token: EmailValidationToken = Uuid::new_v4().into();
        self.repository
            .upsert_pending_email_change(executor, user.id, new_email, &validation_token)
            .await?;

        if user.is_testing {
            debug!(
                "Skipping email change verification email for test account {}",
                user.id
            );
            return Ok(false);
        }

        let email_verification_url = format!(
            "{}users/{}/email-verification/{validation_token}",
            self.application_settings.front_base_url, user.id
        )
        .parse()
        .context("Failed to build email change verification URL")?;
        let template = EmailTemplate::EmailVerification {
            first_name: user.first_name.clone(),
            email_verification_url,
        };
        let recipient = User {
            email: Some(new_email.clone()),
            ..user.clone()
        };
        self.mailer
            .read()
            .await
            .send_email(recipient, template, false)
            .await?;
        Ok(false)
    }

    /// False when no email (SMTP) settings are configured.
    pub async fn is_email_enabled(&self) -> bool {
        self.mailer.read().await.is_enabled()
    }

    /// In an OpenID Connect Authorization code flow, the API has fetched the access token and
    /// thus does not need to validate it.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::AUTH_PROVIDER_USER_ID } = tracing::field::Empty,
            { attr::USER_ID } = tracing::field::Empty
        )
    )]
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
        let auth_user_id: AuthUserId = oidc_provider
            .verify_id_token_claims(&id_token, &nonce)?
            .subject()
            .to_string()
            .into();
        let current_span = tracing::Span::current();
        current_span.record(attr::AUTH_PROVIDER_USER_ID, auth_user_id.to_string());

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
        .inspect(|user| {
            current_span.record(attr::USER_ID, user.id.to_string());
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
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
            .request_async(&oidc_provider.traced_http_client())
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
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::AUTH_PROVIDER_USER_ID } = tracing::field::Empty,
            { attr::USER_ID } = tracing::field::Empty
        )
    )]
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
        let current_span = tracing::Span::current();
        current_span.record(attr::AUTH_PROVIDER_USER_ID, auth_user_id.to_string());
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
        .inspect(|user| {
            current_span.record(attr::USER_ID, user.id.to_string());
        })
    }

    /// In an OpenIDConnect flow, this function update the ID token associated with the given auth_user_id
    /// If there no user associated with the given auth_user_id, it creates a new user.
    #[tracing::instrument(level = "debug", skip_all)]
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
                ))?;
            }
        };

        match self
            .repository
            .update_user_auth_id_token(
                executor,
                &oidc_user_auth.auth_user_id,
                &oidc_user_auth.auth_id_token,
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
                    .request_async(&oidc_provider.traced_http_client())
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
                let email: Pii<EmailAddress> = user_infos
                    .email()
                    .context("No email found in user info")?
                    .parse()
                    .context("Invalid email address")?;

                // Check if the email domain is blacklisted
                let domain = email.expose().domain().to_lowercase();
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

    /// Fetch the email address from the OIDC provider's user info endpoint.
    #[tracing::instrument(level = "debug", skip_all)]
    async fn fetch_oidc_user_email(
        &self,
        oidc_provider: &OpenidConnectProvider,
        access_token: AccessToken,
        auth_user_id: &AuthUserId,
    ) -> Result<Pii<EmailAddress>, UniversalInboxError> {
        let user_infos: CoreUserInfoClaims = oidc_provider
            .client
            .user_info(
                access_token,
                Some(SubjectIdentifier::new(auth_user_id.to_string())),
            )
            .context("UserInfo configuration error")?
            .request_async(&oidc_provider.traced_http_client())
            .await
            .context("UserInfo request error")?;

        let email: Pii<EmailAddress> = user_infos
            .email()
            .context("No email found in OIDC user info")?
            .parse()
            .context("Invalid email address from OIDC provider")?;

        Ok(email)
    }

    /// Close the session whose JWT has the `token_id` (`jti`) claim and expires
    /// at `expires_at` (unix seconds), and return where to send the browser.
    ///
    /// The session is revoked server-side first, so a copy of its cookie stops
    /// authenticating. If the revocation cannot be stored, the logout fails
    /// with `SessionStoreUnavailable` (503) and the caller can retry.
    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user_id.to_string()))]
    pub async fn close_session(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        user_auth_kind: Option<UserAuthKind>,
        token_id: &str,
        expires_at: i64,
    ) -> Result<Url, UniversalInboxError> {
        self.session_revocation
            .revoke_session(token_id, expires_at)
            .await
            .map_err(|err| {
                error!("Failed to revoke the session of user {user_id} on logout: {err:?}");
                UniversalInboxError::SessionStoreUnavailable(anyhow!(err))
            })?;

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
                            &oidc_provider.traced_http_client(),
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
                            .get_user_auth(
                                executor,
                                user_id,
                                UserAuthKind::OIDCAuthorizationCodePKCE,
                            )
                            .await?
                            .ok_or_else(|| {
                                anyhow!(
                                    "User with ID {user_id} does not have OIDCAuthorizationCodePKCE auth parameters"
                                )
                            })?;
                        let UserAuth::OIDCAuthorizationCodePKCE(user_auth) = &user_auth else {
                            return Err(anyhow!(
                                "User with ID {user_id} does not have OIDCAuthorizationCodePKCE auth parameters"
                            ))?;
                        };
                        let id_token =
                            CoreIdToken::from_str(user_auth.auth_id_token.expose_secret())
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
    #[tracing::instrument(level = "debug", skip_all)]
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
        fields({ attr::USER_ID } = user.id.to_string())
    )]
    pub async fn register_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user: User,
        user_auth: UserAuth,
    ) -> Result<User, UniversalInboxError> {
        // Registering an already-known address emails its owner too, so both
        // outcomes draw from the same per-address budget, checked before the
        // lookup so the throttled response does not reveal account existence.
        if let Some(email) = &user.email
            && let Some(retry_after_seconds) = self
                .account_rate_limited_for(AccountRateLimitScope::AccountEmail, email)
                .await
        {
            return Err(UniversalInboxError::TooManyRequests {
                retry_after_seconds,
            });
        }

        let mut new_user = self
            .repository
            .create_user(executor, user, user_auth)
            .await?;
        if !self.is_email_enabled().await {
            // No verification email can be sent: trust the registrant.
            let validated_at = Utc::now();
            self.repository
                .mark_email_as_validated(executor, new_user.id, validated_at)
                .await?;
            new_user.email_validated_at = Some(validated_at);
            return Ok(new_user);
        }
        // The budget was consumed above: do not count this email twice.
        self.send_verification_email_to(executor, &new_user, false)
            .await?;
        Ok(new_user)
    }

    /// Register a new local-password account from sign-up credentials.
    ///
    /// Rejects blacklisted email domains with [`UniversalInboxError::Forbidden`].
    /// When the email is already registered, emails its owner instead and
    /// returns `Ok(())` like a successful registration, so the caller's
    /// response does not reveal whether the account exists.
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn register_local_user(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        credentials: Credentials,
    ) -> Result<(), UniversalInboxError> {
        let email_domain = credentials.email.expose().domain().to_lowercase();
        if let Some(rejection_message) = self
            .application_settings
            .security
            .email_domain_blacklist
            .get(&email_domain)
        {
            return Err(UniversalInboxError::Forbidden(rejection_message.clone()));
        }

        let user_auth = UserAuth::Local(Box::new(LocalUserAuth {
            password_hash: self.get_new_password_hash(credentials.password)?,
            password_reset_at: None,
            password_reset_sent_at: None,
        }));

        // The duplicate insert aborts its transaction: run it in a savepoint
        // so the registration attempt email can still use `executor`.
        let mut savepoint = executor
            .begin()
            .await
            .context("Failed to create savepoint while registering user")?;
        let result = self
            .register_user(
                &mut savepoint,
                User::new(None, None, credentials.email.clone()),
                user_auth,
            )
            .await;

        match result {
            Ok(_) => savepoint
                .commit()
                .await
                .context("Failed to release savepoint while registering user")?,
            Err(UniversalInboxError::AlreadyExists { .. }) => {
                savepoint
                    .rollback()
                    .await
                    .context("Failed to rollback aborted registration savepoint")?;
                self.send_registration_attempt_email(executor, &credentials.email, false)
                    .await?;
            }
            Err(err) => return Err(err),
        }

        Ok(())
    }

    /// Validate local-password credentials, applying per-account throttling.
    ///
    /// On too many recent requests or failures for the email, returns
    /// [`UniversalInboxError::TooManyLoginAttempts`] (→ 429) before the password
    /// is even checked. On a wrong password it records the failure (locking the
    /// account with exponential backoff past the threshold, emailing the owner
    /// once per lock episode) and returns a generic `Unauthorized` that does not
    /// reveal whether the account exists. A correct password resets the counter.
    ///
    /// Throttle (Redis) errors fail open, a deliberate choice: the per-IP
    /// limiter still applies and we prefer availability over locking everyone
    /// out during a Redis outage. They are logged at `error` level so the
    /// outage is alerted on. Session checks, by contrast, fail closed (see
    /// [`UserService::is_session_active`]).
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = tracing::field::Empty)
    )]
    pub async fn validate_credentials(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        credentials: Credentials,
    ) -> Result<User, UniversalInboxError> {
        let email = credentials.email.clone();

        if let Some(retry_after_seconds) = self
            .account_rate_limited_for(AccountRateLimitScope::LoginRequest, &email)
            .await
        {
            return Err(UniversalInboxError::TooManyLoginAttempts {
                retry_after_seconds,
            });
        }

        if let Some(throttle) = &self.login_throttle {
            match throttle.locked_for(&email).await {
                Ok(Some(retry_after_seconds)) => {
                    return Err(UniversalInboxError::TooManyLoginAttempts {
                        retry_after_seconds,
                    });
                }
                Ok(None) => {}
                Err(err) => error!("Login throttle check failed, allowing attempt: {err:?}"),
            }
        }

        match self.check_password(executor, credentials).await {
            Ok(user) => {
                tracing::Span::current().record(attr::USER_ID, user.id.to_string());
                if let Some(throttle) = &self.login_throttle
                    && let Err(err) = throttle.reset(&email).await
                {
                    warn!("Failed to reset login throttle after successful login: {err:?}");
                }
                Ok(user)
            }
            Err(UniversalInboxError::Unauthorized(_)) => {
                self.record_failed_login(executor, &email).await;
                // Generic message: must not reveal whether the account exists.
                Err(UniversalInboxError::Unauthorized(anyhow!(
                    "Invalid email address or password"
                )))
            }
            Err(other) => Err(other),
        }
    }

    /// Count one request against the per-account `scope` budget of `email`.
    /// Returns the seconds until the budget refills when it is exhausted. Like
    /// the login lockout, Redis errors fail open (the per-IP limiter still
    /// applies).
    async fn account_rate_limited_for(
        &self,
        scope: AccountRateLimitScope,
        email: &Pii<EmailAddress>,
    ) -> Option<u64> {
        let throttle = self.login_throttle.as_ref()?;
        match throttle.consume_request(scope, email).await {
            Ok(retry_after_seconds) => retry_after_seconds,
            Err(err) => {
                error!("Account rate limit check failed, allowing request: {err:?}");
                None
            }
        }
    }

    /// Record a failed attempt with the throttle and, if it newly locked the
    /// account, email the (real) owner once. Best-effort: throttle/email errors
    /// are logged, never surfaced, so a failed login still returns its 401.
    async fn record_failed_login(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email: &Pii<EmailAddress>,
    ) {
        let Some(throttle) = &self.login_throttle else {
            return;
        };
        match throttle.record_failure(email).await {
            Ok(outcome) if outcome.newly_locked => {
                let dry_run = self.application_settings.dry_run;
                if let Err(err) = self
                    .send_account_lockout_email(executor, email, dry_run)
                    .await
                {
                    warn!("Failed to send account lockout email: {err:?}");
                }
            }
            Ok(_) => {}
            Err(err) => error!("Failed to record failed login attempt: {err:?}"),
        }
    }

    /// Constant-time password check against the stored hash (or a dummy hash
    /// when the email is unknown) so timing does not leak account existence.
    #[tracing::instrument(level = "debug", skip_all)]
    async fn check_password(
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
        // Every new password (registration, reset, adding or changing a local
        // auth method) goes through here: enforce the policy server-side, the
        // SPA's check is only a convenience.
        Password::check_policy(&password.expose_secret().0).map_err(|err| {
            UniversalInboxError::InvalidInputData {
                source: None,
                user_error: err.to_string(),
            }
        })?;
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
        .hash_password(password.expose_secret().0.as_bytes())
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
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn send_verification_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        if !self.is_email_enabled().await {
            return Err(UniversalInboxError::EmailDisabled);
        }
        let Some(user) = self.repository.get_user(executor, user_id).await? else {
            return Ok(());
        };
        // Resends draw from the same per-address budget as registration and
        // password reset, so one account cannot flood an address.
        if let Some(email) = &user.email
            && let Some(retry_after_seconds) = self
                .account_rate_limited_for(AccountRateLimitScope::AccountEmail, email)
                .await
        {
            return Err(UniversalInboxError::TooManyRequests {
                retry_after_seconds,
            });
        }

        self.send_verification_email_to(executor, &user, dry_run)
            .await
    }

    /// Send the verification email without consuming the per-address budget,
    /// for callers that already consumed it in the same request.
    async fn send_verification_email_to(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user: &User,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        let user_id = user.id;
        // Skip sending verification email for test accounts
        if user.is_testing {
            debug!("Skipping verification email for test account {user_id}");
            return Ok(());
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
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn verify_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        email_validation_token: EmailValidationToken,
    ) -> Result<(), UniversalInboxError> {
        if let Some(pending_change) = self
            .repository
            .get_pending_email_change(executor, user_id)
            .await?
            && pending_change.validation_token == email_validation_token
        {
            self.repository
                .delete_pending_email_change(executor, user_id)
                .await?;
            if pending_change.requested_at + TimeDelta::hours(EMAIL_CHANGE_VALIDITY_HOURS)
                < Utc::now()
            {
                return Err(UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error:
                        "This email change link has expired, please request the change again"
                            .to_string(),
                });
            }
            if !self
                .repository
                .apply_verified_email_change(executor, user_id, &pending_change.new_email)
                .await?
            {
                return Err(UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "This email address cannot be used for this account".to_string(),
                });
            }
            return Ok(());
        }

        let stored_email_validation_token = self
            .repository
            .get_user_email_validation_token(executor, user_id)
            .await?;

        match stored_email_validation_token {
            Some(token) if token == email_validation_token => {
                let email_validation_sent_at = self
                    .repository
                    .get_user(executor, user_id)
                    .await?
                    .and_then(|user| user.email_validation_sent_at);
                let validity = TimeDelta::hours(i64::from(
                    self.application_settings
                        .security
                        .email_verification_token_validity_in_hours,
                ));
                let now = Utc::now();
                // A token without a send date predates this check: treat it as
                // expired rather than valid forever.
                let is_expired =
                    email_validation_sent_at.is_none_or(|sent_at| sent_at + validity < now);
                if is_expired {
                    return Err(UniversalInboxError::InvalidInputData {
                        source: None,
                        user_error: "This email verification link has expired, please request a new verification email".to_string(),
                    });
                }

                self.repository
                    .mark_email_as_validated(executor, user_id, now)
                    .await?;
                Ok(())
            }
            _ => Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Invalid email validation token for user {user_id}"),
            }),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn send_password_reset_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email_address: Pii<EmailAddress>,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        if !self.is_email_enabled().await {
            return Err(UniversalInboxError::EmailDisabled);
        }
        // Checked before the lookup: the budget is consumed and the throttled
        // response identical whether or not the address has an account.
        if let Some(retry_after_seconds) = self
            .account_rate_limited_for(AccountRateLimitScope::AccountEmail, &email_address)
            .await
        {
            return Err(UniversalInboxError::TooManyRequests {
                retry_after_seconds,
            });
        }

        // Skip sending password reset email for test accounts
        let user = self
            .repository
            .get_user_by_email(executor, &email_address)
            .await?;
        if let Some(user) = user
            && user.is_testing
        {
            debug!("Skipping password reset email for test account {}", user.id);
            return Ok(());
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
                // No personal data in telemetry: the address is not logged.
                warn!("No user found for the password reset email address");
            }
            _ => {
                error!("User not updated while resetting password, should not happen");
            }
        }

        Ok(())
    }

    /// Notify an account owner that their account was temporarily locked after
    /// too many failed login attempts. Best-effort and silent for unknown /
    /// test accounts: the login handler must return an identical generic
    /// response regardless, so this never reveals account existence.
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn send_account_lockout_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email: &Pii<EmailAddress>,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        let user = self.repository.get_user_by_email(executor, email).await?;

        let Some(user) = user else {
            return Ok(());
        };
        if user.is_testing {
            debug!(
                "Skipping account lockout email for test account {}",
                user.id
            );
            return Ok(());
        }

        let login_url = format!("{}login", self.application_settings.front_base_url)
            .parse()
            .context("Failed to build login URL")?;

        let template = EmailTemplate::AccountLockout {
            first_name: user.first_name.clone(),
            login_url,
        };
        self.mailer
            .read()
            .await
            .send_email(user, template, dry_run)
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn send_registration_attempt_email(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        email: &Pii<EmailAddress>,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        let user = self.repository.get_user_by_email(executor, email).await?;

        let Some(user) = user else {
            return Ok(());
        };
        if user.is_testing {
            debug!(
                "Skipping registration attempt email for test account {}",
                user.id
            );
            return Ok(());
        }

        let login_url = format!("{}login", self.application_settings.front_base_url)
            .parse()
            .context("Failed to build login URL")?;

        let template = EmailTemplate::RegistrationAttemptOnExistingAccount {
            first_name: user.first_name.clone(),
            login_url,
            password_reset_url: format!(
                "{}password-reset",
                self.application_settings.front_base_url
            )
            .parse()
            .context("Failed to build password reset URL")?,
        };
        self.mailer
            .read()
            .await
            .send_email(user, template, dry_run)
            .await?;

        Ok(())
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
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
                Some((
                    password_reset_token.clone(),
                    Utc::now() - TimeDelta::minutes(PASSWORD_RESET_VALIDITY_MINUTES),
                )),
            )
            .await?;

        match updated_user {
            UpdateStatus {
                result: Some(_), ..
            } => {
                self.revoke_sessions(user_id).await;
                Ok(())
            }
            UpdateStatus { result: None, .. } => Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!("Invalid password reset token for user {user_id}"),
            }),
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn set_password(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        new_password: SecretBox<Password>,
    ) -> Result<(), UniversalInboxError> {
        let new_password_hash = self.get_new_password_hash(new_password)?;
        let updated_user = self
            .repository
            .update_password(executor, user_id, new_password_hash, None)
            .await?;

        match updated_user {
            UpdateStatus {
                result: Some(_), ..
            } => {
                self.revoke_sessions(user_id).await;
                Ok(())
            }
            UpdateStatus { result: None, .. } => Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: format!(
                    "Failed to set password for user {user_id}: no Local auth found"
                ),
            }),
        }
    }

    /// Change the password of an authenticated user after re-checking their
    /// current one, then sign out their other sessions and notify them by
    /// email. The current password check shares the login throttle, so it
    /// cannot be used to brute-force the password either.
    ///
    /// A wrong current password is reported as invalid input, not
    /// `Unauthorized`: a 401 would clear the session cookie and log the user
    /// out.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn change_password(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        password_change: PasswordChange,
    ) -> Result<(), UniversalInboxError> {
        let user = self
            .repository
            .get_user(executor, user_id)
            .await?
            .ok_or_else(|| UniversalInboxError::ItemNotFound(format!("Unknown user {user_id}")))?;
        let (Some(email), Some(UserAuth::Local(_))) = (
            user.email.clone(),
            self.repository
                .get_user_auth(executor, user_id, UserAuthKind::Local)
                .await?,
        ) else {
            return Err(UniversalInboxError::UnsupportedAction(
                "Cannot change the password of an account without password authentication"
                    .to_string(),
            ));
        };

        let PasswordChange {
            current_password,
            new_password,
        } = password_change;
        if current_password.expose_secret().0 == new_password.expose_secret().0 {
            return Err(UniversalInboxError::InvalidInputData {
                source: None,
                user_error: "The new password must be different from the current one".to_string(),
            });
        }

        match self
            .validate_credentials(
                executor,
                Credentials {
                    email,
                    password: current_password,
                },
            )
            .await
        {
            Ok(_) => {}
            Err(UniversalInboxError::Unauthorized(_)) => {
                return Err(UniversalInboxError::InvalidInputData {
                    source: None,
                    user_error: "The current password is incorrect".to_string(),
                });
            }
            Err(err) => return Err(err),
        }

        self.set_password(executor, user_id, new_password).await?;

        let dry_run = self.application_settings.dry_run;
        if let Err(err) = self.send_password_changed_email(user, dry_run).await {
            warn!("Failed to send password changed email: {err:?}");
        }

        Ok(())
    }

    /// Sign out every session of `user_id` issued before now. Best-effort, like
    /// the login throttle: a Redis failure is logged and must not prevent the
    /// password update that triggered it.
    async fn revoke_sessions(&self, user_id: UserId) {
        if let Err(err) = self
            .session_revocation
            .revoke_sessions(user_id, Utc::now())
            .await
        {
            error!("Failed to revoke the sessions of user {user_id}: {err:?}");
        }
    }

    /// Whether the session JWT of `user_id` with the `token_id` (`jti`) claim,
    /// issued at `issued_at` (unix seconds), is still valid. Fails closed: a
    /// Redis error is returned as `SessionStoreUnavailable` (503) rather than
    /// letting a possibly revoked session through.
    pub async fn is_session_active(
        &self,
        user_id: UserId,
        token_id: &str,
        issued_at: i64,
    ) -> Result<bool, UniversalInboxError> {
        self.session_revocation
            .is_session_active(user_id, token_id, issued_at)
            .await
            .map_err(|err| {
                error!("Session revocation check failed, rejecting session: {err:?}");
                UniversalInboxError::SessionStoreUnavailable(anyhow!(err))
            })
    }

    #[tracing::instrument(level = "debug", skip_all, fields({ attr::USER_ID } = user.id.to_string()))]
    async fn send_password_changed_email(
        &self,
        user: User,
        dry_run: bool,
    ) -> Result<(), UniversalInboxError> {
        if user.is_testing {
            debug!(
                "Skipping password changed email for test account {}",
                user.id
            );
            return Ok(());
        }

        let template = EmailTemplate::PasswordChanged {
            first_name: user.first_name.clone(),
            password_reset_url: format!(
                "{}password-reset",
                self.application_settings.front_base_url
            )
            .parse()
            .context("Failed to build password reset URL")?,
        };
        self.mailer
            .read()
            .await
            .send_email(user, template, dry_run)
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn start_passkey_registration(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        username: &Username,
    ) -> Result<(UserId, CreationChallengeResponse, PasskeyRegistration), UniversalInboxError> {
        // Security (universal-inbox-bkj.31): Avoid leaking whether the
        // username is already registered (and the existing user's UserId
        // UUID) from this unauthenticated endpoint. Regardless of whether
        // the username exists, generate a fresh ephemeral `UserId` (never
        // the real one) and mint a real-shaped `CreationChallengeResponse`
        // bound to it. The caller stores the ephemeral id in the session
        // and in Redis; if the ceremony is later completed the database
        // unique constraint on `user_auth.username` will reject the
        // duplicate at `finish_passkey_registration` time. The repository
        // maps that rejection to a generic `UniversalInboxError::Conflict`
        // (see Repository::create_user_auth), so the finish response is a
        // 409 with a user-facing "username is taken" message that never
        // exposes the Postgres constraint name, raw sqlx error text, or
        // the ephemeral UserId UUID. The real `UserId` of the existing
        // account is never returned to the client and the start response
        // shape (status, headers, body) is indistinguishable from the
        // fresh-username path.
        let _ = self
            .repository
            .get_user_auth_by_username(executor, username)
            .await?;
        let user_id: UserId = Uuid::new_v4().into();
        let (creation_challenge_response, passkey_registration) = self
            .webauthn
            .start_passkey_registration(user_id.0, username.0.as_str(), username.0.as_str(), None)
            .context("Failed to start Passkey registration")?;
        Ok((user_id, creation_challenge_response, passkey_registration))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            { attr::USER_USERNAME } = username.to_string(),
            { attr::USER_ID } = user_id.to_string(),
        )
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
            .context("Failed to finish Passkey registration")?;

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

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn start_passkey_authentication(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        username: &Username,
    ) -> Result<(UserId, RequestChallengeResponse, PasskeyAuthentication), UniversalInboxError>
    {
        // Security (universal-inbox-bkj.31): Avoid leaking whether the
        // username is registered. If we find a matching passkey user we
        // generate a real authentication challenge bound to their
        // credential; if not, we mint an indistinguishable challenge
        // bound to a fresh ephemeral UserId with no allowed credentials.
        // The HTTP status and body shape are identical in both branches;
        // the only intentional residual signal is the `allow_credentials`
        // list (real for known users, empty for unknown users) — this is
        // the simpler trade-off explicitly accepted by the issue, since
        // a deterministic synthetic credential is non-trivial to forge.
        // No username or UserId text is ever echoed back to the client.
        let lookup = self
            .repository
            .get_user_auth_by_username(executor, username)
            .await?;
        let (user_id, allowed) = match lookup {
            Some((UserAuth::Passkey(passkey_user_auth), user_id)) => {
                (user_id, vec![passkey_user_auth.passkey])
            }
            // Unknown username, or a user_auth row that is not a passkey:
            // mint a fake challenge under a fresh ephemeral UserId so the
            // outcome is shape-identical to the success path. The
            // ceremony cannot be completed (no matching state can be
            // produced from any real credential) but the *start* endpoint
            // response is non-enumerable.
            _ => (UserId::from(Uuid::new_v4()), Vec::new()),
        };

        let (request_challenge_response, passkey_authentication) = self
            .webauthn
            .start_passkey_authentication(&allowed)
            .context("Failed to start Passkey authentication")?;

        Ok((user_id, request_challenge_response, passkey_authentication))
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
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

        let Some(user_auth) = self
            .repository
            .get_user_auth(executor, user_id, UserAuthKind::Passkey)
            .await?
        else {
            return Err(UniversalInboxError::ItemNotFound(format!(
                "No passkey auth found for user {user_id}"
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

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn get_user_preferences(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
    ) -> Result<Option<UserPreferences>, UniversalInboxError> {
        self.repository
            .get_user_preferences(executor, user_id)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields({ attr::USER_ID } = user_id.to_string())
    )]
    pub async fn patch_user_preferences(
        &self,
        executor: &mut Transaction<'_, Postgres>,
        user_id: UserId,
        patch: &UserPreferencesPatch,
    ) -> Result<UserPreferences, UniversalInboxError> {
        self.repository
            .create_or_update_user_preferences(executor, user_id, patch)
            .await
    }
}
