use anyhow::Result;
use dioxus::prelude::*;
use email_address::EmailAddress;

use futures_util::StreamExt;
use log::error;
use reqwest::Method;
use secrecy::SecretBox;
use url::Url;
use webauthn_rs_proto::*;

use universal_inbox::{
    SuccessResponse,
    auth::CloseSessionResponse,
    user::{
        Credentials, EmailValidationToken, Password, PasswordResetToken, RegisterUserParameters,
        UserContext, UserId, UserPatch, Username,
    },
};

use crate::{
    model::UniversalInboxUIModel,
    services::{api::call_api, crisp::unload_crisp},
    utils::{create_navigator_credentials, get_navigator_credentials, redirect_to},
};

pub enum UserCommand {
    GetUser,
    RegisterUser(RegisterUserParameters),
    Login(Credentials),
    Logout,
    ResendVerificationEmail,
    VerifyEmail(UserId, EmailValidationToken),
    SendPasswordResetEmail(EmailAddress),
    ResetPassword(UserId, PasswordResetToken, SecretBox<Password>),
    RegisterPasskey(Username),
    LoginPasskey(Username),
    UpdateUser(UserPatch),
}

pub static CONNECTED_USER: GlobalSignal<Option<UserContext>> = Signal::global(|| None);

pub async fn user_service(
    mut rx: UnboundedReceiver<UserCommand>,
    api_base_url: Url,
    mut connected_user: Signal<Option<UserContext>>,
    mut ui_model: Signal<UniversalInboxUIModel>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(UserCommand::GetUser) => {
                get_user(&api_base_url, connected_user, ui_model).await;
            }

            Some(UserCommand::RegisterUser(parameters)) => {
                let result: Result<UserContext> = call_api(
                    Method::POST,
                    &api_base_url,
                    "users",
                    Some(parameters),
                    Some(ui_model),
                )
                .await;

                match result {
                    Ok(user_context) => {
                        connected_user.write().replace(user_context);
                    }
                    Err(err) => {
                        ui_model.write().error_message = Some(err.to_string());
                    }
                };
            }

            Some(UserCommand::Login(credentials)) => {
                ui_model.write().error_message = None;
                let result: Result<UserContext> = call_api(
                    Method::POST,
                    &api_base_url,
                    "users/me",
                    Some(credentials),
                    Some(ui_model),
                )
                .await;

                match result {
                    Ok(user_context) => {
                        connected_user.write().replace(user_context);
                    }
                    Err(err) => {
                        ui_model.write().error_message = Some(err.to_string());
                    }
                };
            }
            Some(UserCommand::Logout) => {
                unload_crisp();
                let result: Result<CloseSessionResponse> = call_api(
                    Method::DELETE,
                    &api_base_url,
                    "auth/session",
                    None::<i32>,
                    Some(ui_model),
                )
                .await;

                if let Ok(CloseSessionResponse { logout_url }) = result {
                    let _ = redirect_to(logout_url.as_str());
                };
            }
            Some(UserCommand::ResendVerificationEmail) => {
                let result: Result<SuccessResponse> = call_api(
                    Method::POST,
                    &api_base_url,
                    "users/me/email-verification",
                    None::<i32>,
                    Some(ui_model),
                )
                .await;

                match result {
                    Ok(SuccessResponse { message, .. }) => {
                        ui_model.write().confirmation_message = Some(message);
                    }
                    Err(err) => {
                        ui_model.write().error_message = Some(err.to_string());
                    }
                };
            }
            Some(UserCommand::VerifyEmail(user_id, email_validation_token)) => {
                let result: Result<SuccessResponse> = call_api(
                    Method::GET,
                    &api_base_url,
                    format!("users/{user_id}/email-verification/{email_validation_token}").as_str(),
                    None::<i32>,
                    None,
                )
                .await;

                match result {
                    Ok(SuccessResponse { message, .. }) => {
                        ui_model.write().confirmation_message = Some(message);
                        // Refresh user as it should now have a validated email and either redirected to the
                        // to the app if it has a logged session or to the login form otherwise
                        get_user(&api_base_url, connected_user, ui_model).await
                    }
                    Err(err) => {
                        ui_model.write().error_message = Some(err.to_string());
                    }
                };
            }

            Some(UserCommand::SendPasswordResetEmail(email_address)) => {
                let result: Result<SuccessResponse> = call_api(
                    Method::POST,
                    &api_base_url,
                    "users/password-reset",
                    Some(email_address),
                    None,
                )
                .await;

                match result {
                    Ok(SuccessResponse { message, .. }) => {
                        ui_model.write().confirmation_message = Some(message);
                    }
                    Err(err) => {
                        ui_model.write().error_message = Some(err.to_string());
                    }
                };
            }
            Some(UserCommand::ResetPassword(user_id, password_reset_token, new_password)) => {
                let result: Result<SuccessResponse> = call_api(
                    Method::POST,
                    &api_base_url,
                    format!("users/{user_id}/password-reset/{password_reset_token}").as_str(),
                    Some(new_password),
                    None,
                )
                .await;

                match result {
                    Ok(SuccessResponse { message, .. }) => {
                        ui_model.write().confirmation_message = Some(message);
                        // Refresh user as it should now be authenticated
                        get_user(&api_base_url, connected_user, ui_model).await
                    }
                    Err(err) => {
                        ui_model.write().error_message = Some(err.to_string());
                    }
                };
            }
            Some(UserCommand::LoginPasskey(username)) => {
                start_passkey_authentication(username, &api_base_url, connected_user, ui_model)
                    .await;
            }
            Some(UserCommand::RegisterPasskey(username)) => {
                start_passkey_registration(username, &api_base_url, connected_user, ui_model).await;
            }
            Some(UserCommand::UpdateUser(patch)) => {
                let result: Result<universal_inbox::user::User> = call_api(
                    Method::PATCH,
                    &api_base_url,
                    "users/me",
                    Some(patch),
                    Some(ui_model),
                )
                .await;

                match result {
                    Ok(user) => {
                        if let Some(ref mut user_context) = *connected_user.write() {
                            user_context.user = user;
                        }
                    }
                    Err(err) => {
                        ui_model.write().error_message = Some(err.to_string());
                    }
                };
            }
            None => {}
        }
    }
}

async fn get_user(
    api_base_url: &Url,
    mut connected_user: Signal<Option<UserContext>>,
    ui_model: Signal<UniversalInboxUIModel>,
) {
    let result: Result<UserContext> = call_api(
        Method::GET,
        api_base_url,
        "users/me",
        None::<i32>,
        Some(ui_model),
    )
    .await;

    match result {
        Ok(user_context) => {
            *connected_user.write() = Some(user_context);
        }
        Err(err) => {
            error!("Failed to get current user: {err}");
        }
    }
}

async fn start_passkey_registration(
    username: Username,
    api_base_url: &Url,
    connected_user: Signal<Option<UserContext>>,
    ui_model: Signal<UniversalInboxUIModel>,
) {
    let result: Result<CreationChallengeResponse> = call_api(
        Method::POST,
        api_base_url,
        "users/passkeys/registration/start",
        Some(username),
        None,
    )
    .await;

    let c_options: web_sys::CredentialCreationOptions = match result {
        Ok(ccr) => ccr.into(),
        Err(err) => {
            error!("Failed to start Passkey registration: {err}");
            return;
        }
    };

    let rpkc = match create_navigator_credentials(c_options).await {
        Ok(w_rpkc) => RegisterPublicKeyCredential::from(w_rpkc),
        Err(err) => {
            error!("Failed to create public key for Passkey authentication: {err}");
            return;
        }
    };
    finish_passkey_registration(rpkc, api_base_url, connected_user, ui_model).await;
}

async fn finish_passkey_registration(
    register_credentials: RegisterPublicKeyCredential,
    api_base_url: &Url,
    mut connected_user: Signal<Option<UserContext>>,
    mut ui_model: Signal<UniversalInboxUIModel>,
) {
    let result: Result<UserContext> = call_api(
        Method::POST,
        api_base_url,
        "users/passkeys/registration/finish",
        Some(register_credentials),
        Some(ui_model),
    )
    .await;

    match result {
        Ok(user_context) => {
            connected_user.write().replace(user_context);
        }
        Err(err) => {
            ui_model.write().error_message = Some(err.to_string());
        }
    };
}

async fn start_passkey_authentication(
    username: Username,
    api_base_url: &Url,
    connected_user: Signal<Option<UserContext>>,
    ui_model: Signal<UniversalInboxUIModel>,
) {
    let result: Result<RequestChallengeResponse> = call_api(
        Method::POST,
        api_base_url,
        "users/passkeys/authentication/start",
        Some(username),
        None,
    )
    .await;

    let c_options: web_sys::CredentialRequestOptions = match result {
        Ok(rcr) => rcr.into(),
        Err(err) => {
            error!("Failed to start Passkey authentication: {err}");
            return;
        }
    };

    let pkc = match get_navigator_credentials(c_options).await {
        Ok(w_rpkc) => PublicKeyCredential::from(w_rpkc),
        Err(err) => {
            error!("Failed to get public key for Passkey authentication: {err}");
            return;
        }
    };
    finish_passkey_authentication(pkc, api_base_url, connected_user, ui_model).await;
}

async fn finish_passkey_authentication(
    credentials: PublicKeyCredential,
    api_base_url: &Url,
    mut connected_user: Signal<Option<UserContext>>,
    mut ui_model: Signal<UniversalInboxUIModel>,
) {
    let result: Result<UserContext> = call_api(
        Method::POST,
        api_base_url,
        "users/passkeys/authentication/finish",
        Some(credentials),
        Some(ui_model),
    )
    .await;

    match result {
        Ok(user_context) => {
            connected_user.write().replace(user_context);
        }
        Err(err) => {
            ui_model.write().error_message = Some(err.to_string());
        }
    };
}
