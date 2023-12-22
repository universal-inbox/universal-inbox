use std::collections::HashMap;

use anyhow::anyhow;
use secrecy::Secret;
use universal_inbox::user::{Credentials, RegisterUserParameters};

pub struct FormValues(pub HashMap<String, Vec<String>>);

impl TryFrom<FormValues> for Credentials {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let email = form_values
            .0
            .get("email")
            .ok_or_else(|| anyhow!("email is required"))?
            .first()
            .ok_or_else(|| anyhow!("email is required"))?
            .parse()?;

        let password = form_values
            .0
            .get("password")
            .ok_or_else(|| anyhow!("password is required"))?
            .first()
            .ok_or_else(|| anyhow!("password is required"))?
            .parse()?;

        Ok(Self {
            email,
            password: Secret::new(password),
        })
    }
}

impl TryFrom<FormValues> for RegisterUserParameters {
    type Error = anyhow::Error;

    fn try_from(form_values: FormValues) -> Result<Self, Self::Error> {
        let first_name = form_values
            .0
            .get("first_name")
            .ok_or_else(|| anyhow!("first_name is required"))?
            .first()
            .ok_or_else(|| anyhow!("first_name is required"))?
            .to_string();

        let last_name = form_values
            .0
            .get("last_name")
            .ok_or_else(|| anyhow!("last_name is required"))?
            .first()
            .ok_or_else(|| anyhow!("last_name is required"))?
            .to_string();

        Self::try_new(first_name, last_name, form_values.try_into()?)
    }
}
