use anyhow::Context;
use webauthn_rs::prelude::*;

use crate::universal_inbox::UniversalInboxError;

pub fn build_webauthn(front_base_url: &Url) -> Result<Webauthn, UniversalInboxError> {
    let rp_id = front_base_url
        .domain()
        .with_context(|| "Unable to extract domain from URL {front_base_url}")?;
    Ok(WebauthnBuilder::new(rp_id, front_base_url)
        .context("Invalid Webauthn configuration")?
        .rp_name("Universal Inbox")
        .build()
        .context("Invalid Webauthn configuration")?)
}
