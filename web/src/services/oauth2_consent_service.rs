use anyhow::Result;
use reqwest::Method;
use url::Url;

use universal_inbox::auth::oauth2::{
    OAuth2ConsentDecision, OAuth2ConsentRequest, OAuth2ConsentResponse, OAuth2ConsentSubmission,
};

use crate::services::api::call_api;

pub async fn fetch_consent_request(
    api_base_url: &Url,
    request_id: &str,
) -> Result<OAuth2ConsentRequest> {
    // `request_id` is a server-generated UUID, so a raw substitution into the
    // query string is safe; we percent-encode minimal chars defensively.
    let encoded = url::form_urlencoded::byte_serialize(request_id.as_bytes()).collect::<String>();
    call_api(
        Method::GET,
        api_base_url,
        &format!("oauth2/authorize/consent?request_id={encoded}"),
        None::<i32>,
        None,
    )
    .await
}

pub async fn submit_consent_decision(
    api_base_url: &Url,
    request_id: &str,
    csrf_token: &str,
    decision: OAuth2ConsentDecision,
) -> Result<OAuth2ConsentResponse> {
    let body = OAuth2ConsentSubmission {
        request_id: request_id.to_string(),
        csrf_token: csrf_token.to_string(),
        decision,
    };
    call_api(
        Method::POST,
        api_base_url,
        "oauth2/authorize/consent",
        Some(body),
        None,
    )
    .await
}
