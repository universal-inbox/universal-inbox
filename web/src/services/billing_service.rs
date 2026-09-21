//! Frontend client for the optional Stripe billing API.
//!
//! Treats HTTP 404 from `/api/billing/*` as "billing is disabled on this
//! instance" — the UI hides plan badges and the upgrade modal when
//! `BILLING_STATE.read()` is `None`. Self-hosted Universal Inbox instances
//! never serve billing endpoints, so 404 is the natural signal.

use dioxus::prelude::*;
use futures_util::StreamExt;
use log::{debug, error};
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde_json::json;
use url::Url;

use crate::services::api::{API_CLIENT, call_api};

/// The `/billing/me` response shape now lives in the shared crate
/// (`universal_inbox::billing::BillingStateResponse`) so API and web can't
/// drift. Re-exported under the historical `BillingState` name to avoid
/// churning every call site.
pub use universal_inbox::billing::BillingStateResponse as BillingState;

/// Three-state value: `Unknown` until the first refresh completes,
/// `Disabled` when the API returned 404 (self-hosted instance),
/// `Enabled(state)` when billing is active.
#[derive(Debug, Clone, PartialEq)]
pub enum BillingAvailability {
    Unknown,
    Disabled,
    Enabled(BillingState),
}

impl BillingAvailability {
    /// Returns `Some(state)` when billing is active, otherwise `None`.
    /// Useful for component-level `if let Some(state) = ...` guards.
    #[allow(dead_code)]
    pub fn as_state(&self) -> Option<&BillingState> {
        match self {
            BillingAvailability::Enabled(s) => Some(s),
            _ => None,
        }
    }
}

pub static BILLING_STATE: GlobalSignal<BillingAvailability> =
    Signal::global(|| BillingAvailability::Unknown);

/// Payload carried by [`UPGRADE_TRIGGER`] when the API returns HTTP 402 from
/// any endpoint. The web app's modal subscribes to this signal and pops the
/// upgrade flow without the calling service having to know about the modal.
#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeTrigger {
    /// Stable error code from the API response body (e.g.
    /// `free_plan_integration_limit_reached`). The modal uses it to pick the
    /// right headline copy.
    pub code: String,
    pub message: Option<String>,
}

pub static UPGRADE_TRIGGER: GlobalSignal<Option<UpgradeTrigger>> = Signal::global(|| None);

#[derive(Debug)]
pub enum BillingCommand {
    /// Refresh the persisted billing state from `/api/billing/me`.
    Refresh,
    /// Start a Stripe Checkout session and redirect the browser.
    OpenCheckout { success_url: Url, cancel_url: Url },
    /// Start a Stripe Customer Portal session and redirect the browser.
    OpenPortal { return_url: Url },
}

#[derive(Deserialize)]
struct SessionUrlResponse {
    url: Url,
}

pub async fn billing_service(
    mut rx: UnboundedReceiver<BillingCommand>,
    api_base_url: Url,
    mut billing_state: Signal<BillingAvailability>,
) {
    // Kick off an initial fetch so the sidebar plan pill renders right after
    // login without waiting for the user to navigate.
    refresh(&api_base_url, &mut billing_state).await;

    while let Some(command) = rx.next().await {
        match command {
            BillingCommand::Refresh => refresh(&api_base_url, &mut billing_state).await,
            BillingCommand::OpenCheckout {
                success_url,
                cancel_url,
            } => {
                if let Some(url) = create_session(
                    &api_base_url,
                    "billing/checkout-session",
                    &json!({ "success_url": success_url, "cancel_url": cancel_url }),
                )
                .await
                {
                    redirect_to(&url);
                }
            }
            BillingCommand::OpenPortal { return_url } => {
                if let Some(url) = create_session(
                    &api_base_url,
                    "billing/portal-session",
                    &json!({ "return_url": return_url }),
                )
                .await
                {
                    redirect_to(&url);
                }
            }
        }
    }
}

async fn refresh(api_base_url: &Url, billing_state: &mut Signal<BillingAvailability>) {
    let url = match api_base_url.join("billing/me") {
        Ok(u) => u,
        Err(err) => {
            error!("Cannot build billing/me URL: {err}");
            return;
        }
    };

    let response = match API_CLIENT
        .request(Method::GET, url)
        .fetch_credentials_include()
        .send()
        .await
    {
        Ok(r) => r,
        Err(err) => {
            error!("Failed to fetch billing state: {err}");
            return;
        }
    };

    if response.status() == StatusCode::NOT_FOUND {
        // Self-hosted instance: no billing endpoint. Hide all billing UI.
        debug!("/api/billing/me returned 404 → billing disabled on this instance");
        *billing_state.write() = BillingAvailability::Disabled;
        return;
    }

    if !response.status().is_success() {
        error!(
            "/api/billing/me returned unexpected status {}",
            response.status()
        );
        return;
    }

    match response.json::<BillingState>().await {
        Ok(state) => {
            *billing_state.write() = BillingAvailability::Enabled(state);
        }
        Err(err) => {
            error!("Failed to deserialize billing state: {err}");
        }
    }
}

async fn create_session(api_base_url: &Url, path: &str, body: &serde_json::Value) -> Option<Url> {
    // `call_api` already does credentials, the content-type header, the
    // centralized 402 -> upgrade-modal trigger (a 402 here means e.g. a
    // portal-session with no Stripe customer yet), version check, and error
    // extraction — so this only maps the result to the historical `Option<Url>`.
    match call_api::<SessionUrlResponse, _>(Method::POST, api_base_url, path, Some(body), None)
        .await
    {
        Ok(session) => Some(session.url),
        Err(err) => {
            error!("Failed to create billing session at {path}: {err}");
            None
        }
    }
}

/// The checkout every caller means: land back on the billing page, saying how
/// it went. Built here so the buttons that send it cannot drift apart.
pub fn open_checkout() -> BillingCommand {
    BillingCommand::OpenCheckout {
        success_url: current_url("/billing?checkout=success"),
        cancel_url: current_url("/billing?checkout=cancel"),
    }
}

/// The Stripe billing portal, returning to the billing page.
pub fn open_portal() -> BillingCommand {
    BillingCommand::OpenPortal {
        return_url: current_url("/billing"),
    }
}

/// Build an absolute URL pointing at `path` on the current origin, used to
/// fill Stripe `success_url` / `cancel_url` / `return_url` so Stripe always
/// returns the user to Universal Inbox. Single shared definition — the billing
/// page, the upgrade modal, and the banner strip all call this so their
/// post-checkout redirects can't drift apart.
pub fn current_url(path: &str) -> Url {
    let base = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| "http://localhost".to_string());
    Url::parse(&base)
        .and_then(|b| b.join(path))
        .unwrap_or_else(|_| Url::parse("http://localhost/").expect("static URL parses"))
}

fn redirect_to(url: &Url) {
    // Best-effort full-page redirect; we don't expect to come back so we don't
    // need to surface errors past a log.
    if let Some(window) = web_sys::window()
        && let Err(err) = window.location().set_href(url.as_str())
    {
        error!("Failed to redirect to billing session URL: {err:?}");
    }
}
