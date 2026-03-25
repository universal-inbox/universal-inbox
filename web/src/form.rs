use anyhow::anyhow;
use dioxus::prelude::FormValue;
use email_address::EmailAddress;
use secrecy::SecretBox;

use universal_inbox::user::{Credentials, Password, RegisterUserParameters, UserPatch, Username};

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

impl TryFrom<FormValues> for SecretBox<Password> {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let password = form_values
            .get_text("password")
            .ok_or_else(|| anyhow!("password is required"))?
            .parse()?;

        Ok(SecretBox::new(Box::new(password)))
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
