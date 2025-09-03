use std::collections::HashMap;

use anyhow::anyhow;
use dioxus::prelude::FormValue;
use email_address::EmailAddress;
use secrecy::SecretBox;

use universal_inbox::user::{Credentials, Password, RegisterUserParameters, Username};

pub struct FormValues(pub HashMap<String, FormValue>);

impl TryFrom<FormValues> for Credentials {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let email = form_values
            .0
            .get("email")
            .ok_or_else(|| anyhow!("email is required"))?
            .clone()
            .to_vec()
            .first()
            .ok_or_else(|| anyhow!("email is required"))?
            .parse()?;

        let password = form_values
            .0
            .get("password")
            .ok_or_else(|| anyhow!("password is required"))?
            .clone()
            .to_vec()
            .first()
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
            .0
            .get("email")
            .ok_or_else(|| anyhow!("email is required"))?
            .clone()
            .to_vec()
            .first()
            .ok_or_else(|| anyhow!("email is required"))?
            .parse()?;

        Ok(email)
    }
}

impl TryFrom<FormValues> for SecretBox<Password> {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let password = form_values
            .0
            .get("password")
            .ok_or_else(|| anyhow!("password is required"))?
            .clone()
            .to_vec()
            .first()
            .ok_or_else(|| anyhow!("password is required"))?
            .parse()?;

        Ok(SecretBox::new(Box::new(password)))
    }
}

impl TryFrom<FormValues> for Username {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let username = form_values
            .0
            .get("username")
            .ok_or_else(|| anyhow!("username is required"))?
            .clone()
            .to_vec()
            .first()
            .ok_or_else(|| anyhow!("username is required"))?
            .to_owned();

        Ok(Username(username))
    }
}
