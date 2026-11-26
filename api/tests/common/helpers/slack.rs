use chrono::{DateTime, Utc};
use reqwest::{Client, Response};
use ring::hmac;

/// Signing secret matching `api/config/test.toml::[integrations.slack].signing_secret`.
const TEST_SLACK_SIGNING_SECRET: &str = "test-signing-secret-do-not-use-in-prod";

/// POST a Slack push event to `/hooks/slack/events` with a freshly computed
/// `X-Slack-Signature` + `X-Slack-Request-Timestamp` pair. Mirrors the production
/// Slack signing algorithm so tests exercise the same code path real webhooks hit.
pub async fn post_signed_slack_event<T: serde::Serialize>(
    client: &Client,
    api_address: &str,
    payload: &T,
) -> Response {
    post_slack_event_signed_with(
        client,
        api_address,
        payload,
        TEST_SLACK_SIGNING_SECRET,
        Utc::now(),
    )
    .await
}

pub async fn post_slack_event_signed_at<T: serde::Serialize>(
    client: &Client,
    api_address: &str,
    payload: &T,
    signed_at: DateTime<Utc>,
) -> Response {
    post_slack_event_signed_with(
        client,
        api_address,
        payload,
        TEST_SLACK_SIGNING_SECRET,
        signed_at,
    )
    .await
}

pub async fn post_slack_event_signed_with<T: serde::Serialize>(
    client: &Client,
    api_address: &str,
    payload: &T,
    signing_secret: &str,
    signed_at: DateTime<Utc>,
) -> Response {
    let body = serde_json::to_vec(payload).expect("Failed to serialize Slack payload");
    let timestamp = signed_at.timestamp().to_string();
    let signature = compute_slack_signature(signing_secret, &timestamp, &body);

    client
        .post(format!("{api_address}hooks/slack/events"))
        .header("content-type", "application/json")
        .header("X-Slack-Request-Timestamp", timestamp)
        .header("X-Slack-Signature", signature)
        .body(body)
        .send()
        .await
        .expect("Failed to execute request")
}

fn compute_slack_signature(signing_secret: &str, timestamp: &str, body: &[u8]) -> String {
    let mut basestring = Vec::with_capacity(3 + timestamp.len() + 1 + body.len());
    basestring.extend_from_slice(b"v0:");
    basestring.extend_from_slice(timestamp.as_bytes());
    basestring.push(b':');
    basestring.extend_from_slice(body);

    let key = hmac::Key::new(hmac::HMAC_SHA256, signing_secret.as_bytes());
    let tag = hmac::sign(&key, &basestring);
    format!("v0={}", hex::encode(tag.as_ref()))
}

pub async fn post_unsigned_slack_event<T: serde::Serialize>(
    client: &Client,
    api_address: &str,
    payload: &T,
) -> Response {
    client
        .post(format!("{api_address}hooks/slack/events"))
        .json(payload)
        .send()
        .await
        .expect("Failed to execute request")
}
