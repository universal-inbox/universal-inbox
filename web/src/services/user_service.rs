use anyhow::Result;
use dioxus::prelude::*;
use email_address::EmailAddress;
use fermi::{AtomRef, UseAtomRef};
use futures_util::StreamExt;
use log::error;
use reqwest::Method;
use secrecy::Secret;
use url::Url;

use universal_inbox::{
    auth::CloseSessionResponse,
    user::{
        Credentials, EmailValidationToken, Password, PasswordResetToken, RegisterUserParameters,
        User, UserId,
    },
    SuccessResponse,
};

use crate::{model::UniversalInboxUIModel, services::api::call_api, utils::redirect_to};

pub enum UserCommand {
    GetUser,
    RegisterUser(RegisterUserParameters),
    Login(Credentials),
    Logout,
    ResendVerificationEmail,
    VerifyEmail(UserId, EmailValidationToken),
    SendPasswordResetEmail(EmailAddress),
    ResetPassword(UserId, PasswordResetToken, Secret<Password>),
}

pub static CONNECTED_USER: AtomRef<Option<User>> = AtomRef(|_| None);

pub async fn user_service<'a>(
    mut rx: UnboundedReceiver<UserCommand>,
    api_base_url: Url,
    connected_user: UseAtomRef<Option<User>>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
) {
    loop {
        let msg = rx.next().await;
        match msg {
            Some(UserCommand::GetUser) => {
                get_user(&api_base_url, connected_user.clone(), ui_model_ref.clone()).await;
            }

            Some(UserCommand::RegisterUser(parameters)) => {
                let result: Result<User> = call_api(
                    Method::POST,
                    &api_base_url,
                    "users",
                    Some(parameters),
                    Some(ui_model_ref.clone()),
                )
                .await;

                match result {
                    Ok(user) => {
                        connected_user.write().replace(user);
                    }
                    Err(err) => {
                        ui_model_ref.write().error_message = Some(err.to_string());
                    }
                };
            }

            Some(UserCommand::Login(credentials)) => {
                ui_model_ref.write().error_message = None;
                let result: Result<User> = call_api(
                    Method::POST,
                    &api_base_url,
                    "users/me",
                    Some(credentials),
                    Some(ui_model_ref.clone()),
                )
                .await;

                match result {
                    Ok(user) => {
                        connected_user.write().replace(user);
                    }
                    Err(err) => {
                        ui_model_ref.write().error_message = Some(err.to_string());
                    }
                };
            }
            Some(UserCommand::Logout) => {
                let result: Result<CloseSessionResponse> = call_api(
                    Method::DELETE,
                    &api_base_url,
                    "auth/session",
                    None::<i32>,
                    Some(ui_model_ref.clone()),
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
                    Some(ui_model_ref.clone()),
                )
                .await;

                match result {
                    Ok(SuccessResponse { message, .. }) => {
                        ui_model_ref.write().confirmation_message = Some(message);
                    }
                    Err(err) => {
                        ui_model_ref.write().error_message = Some(err.to_string());
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
                        ui_model_ref.write().confirmation_message = Some(message);
                        // Refresh user as it should now have a validated email and either redirected to the
                        // to the app if it has a logged session or to the login form otherwise
                        get_user(&api_base_url, connected_user.clone(), ui_model_ref.clone()).await
                    }
                    Err(err) => {
                        ui_model_ref.write().error_message = Some(err.to_string());
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
                        ui_model_ref.write().confirmation_message = Some(message);
                    }
                    Err(err) => {
                        ui_model_ref.write().error_message = Some(err.to_string());
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
                        ui_model_ref.write().confirmation_message = Some(message);
                        // Refresh user as it should now be authenticated
                        get_user(&api_base_url, connected_user.clone(), ui_model_ref.clone()).await
                    }
                    Err(err) => {
                        ui_model_ref.write().error_message = Some(err.to_string());
                    }
                };
            }
            None => {}
        }
    }
}

async fn get_user(
    api_base_url: &Url,
    connected_user: UseAtomRef<Option<User>>,
    ui_model_ref: UseAtomRef<UniversalInboxUIModel>,
) {
    let result: Result<User> = call_api(
        Method::GET,
        api_base_url,
        "users/me",
        None::<i32>,
        Some(ui_model_ref),
    )
    .await;

    match result {
        Ok(user) => {
            connected_user.write().replace(user);
        }
        Err(err) => {
            error!("Failed to get current user: {err}");
        }
    }
}
