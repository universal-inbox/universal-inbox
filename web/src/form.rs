use anyhow::anyhow;
use dioxus::prelude::FormValue;
use email_address::EmailAddress;
use secrecy::SecretBox;

use universal_inbox::user::{
    Credentials, NewPassword, Password, PasswordChange, RegisterUserParameters, UserPatch, Username,
};

pub struct FormValues(pub Vec<(String, FormValue)>);

impl FormValues {
    fn get_text(&self, name: &str) -> Option<&str> {
        self.0.iter().find_map(|(k, v)| {
            if k == name {
                match v {
                    FormValue::Text(s) => Some(s.as_str()),
                    _ => None,
                }
            } else {
                None
            }
        })
    }
}

impl TryFrom<FormValues> for Credentials {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let email = form_values
            .get_text("email")
            .ok_or_else(|| anyhow!("email is required"))?
            .parse()?;

        let password = form_values
            .get_text("password")
            .ok_or_else(|| anyhow!("password is required"))?
            .parse()?;

        Ok(Self {
            email,
            password: SecretBox::new(Box::new(password)),
        })
    }
}

impl TryFrom<FormValues> for RegisterUserParameters {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        Self::try_new(form_values.try_into()?)
    }
}

impl TryFrom<FormValues> for EmailAddress {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let email = form_values
            .get_text("email")
            .ok_or_else(|| anyhow!("email is required"))?
            .parse()?;

        Ok(email)
    }
}

/// A password being set (reset, new local auth method): the full password
/// policy applies.
impl TryFrom<FormValues> for SecretBox<Password> {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let password: NewPassword = form_values
            .get_text("password")
            .ok_or_else(|| anyhow!("password is required"))?
            .parse()?;

        Ok(SecretBox::new(Box::new(password.into())))
    }
}

impl TryFrom<FormValues> for PasswordChange {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        // The current password was set under whatever policy applied at the
        // time: only the new one is checked against the current policy.
        let current_password = Password(
            form_values
                .get_text("current_password")
                .filter(|password| !password.is_empty())
                .ok_or_else(|| anyhow!("current password is required"))?
                .to_string(),
        );
        let new_password: Password = form_values
            .get_text("new_password")
            .ok_or_else(|| anyhow!("new password is required"))?
            .parse::<NewPassword>()?
            .into();
        let new_password_confirmation = form_values
            .get_text("new_password_confirmation")
            .ok_or_else(|| anyhow!("new password confirmation is required"))?;
        if new_password.0 != new_password_confirmation {
            return Err(anyhow!("the new passwords do not match"));
        }

        Ok(Self {
            current_password: SecretBox::new(Box::new(current_password)),
            new_password: SecretBox::new(Box::new(new_password)),
        })
    }
}

impl TryFrom<FormValues> for Username {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let username = form_values
            .get_text("username")
            .ok_or_else(|| anyhow!("username is required"))?
            .to_owned();

        Ok(Username(username))
    }
}

impl TryFrom<FormValues> for UserPatch {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let first_name = form_values
            .get_text("first_name")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_owned());

        let last_name = form_values
            .get_text("last_name")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_owned());

        let email = form_values
            .get_text("email")
            .filter(|s| !s.is_empty())
            .map(|s| s.parse())
            .transpose()?;

        Ok(UserPatch {
            first_name,
            last_name,
            email,
        })
    }
}
