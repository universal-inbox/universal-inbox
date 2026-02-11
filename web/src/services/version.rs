use anyhow::Context;
use gloo_utils::errors::JsError;
use log::{debug, warn};

use crate::model::{VERSION, VERSION_MISMATCH};

const VERSION_RELOAD_ATTEMPTED_KEY: &str = "version-reload-attempted";

/// Check if the backend version matches the frontend version.
///
/// - If versions match: clear any reload tracking state.
/// - If mismatch and we haven't reloaded yet for this backend version:
///   store the backend version in `sessionStorage` and trigger a cache-busting reload.
/// - If mismatch but we already reloaded once for this backend version:
///   set `VERSION_MISMATCH` signal to show a warning banner instead of reloading again.
pub fn check_version_mismatch(backend_version: &str) {
    let Some(frontend_version) = VERSION else {
        debug!("No frontend VERSION set, skipping version mismatch check");
        return;
    };

    if frontend_version == backend_version {
        debug!("Frontend and backend versions match: {frontend_version}");
        clear_reload_attempted();
        *VERSION_MISMATCH.write() = None;
        return;
    }

    warn!("Version mismatch detected: frontend={frontend_version}, backend={backend_version}");

    if has_reload_been_attempted_for(backend_version) {
        warn!(
            "Already reloaded once for backend version {backend_version}, showing warning instead"
        );
        *VERSION_MISMATCH.write() = Some(backend_version.to_string());
        return;
    }

    set_reload_attempted(backend_version);
    trigger_cache_busting_reload(backend_version);
}

fn get_session_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok().flatten()
}

fn has_reload_been_attempted_for(backend_version: &str) -> bool {
    get_session_storage()
        .and_then(|storage| {
            storage
                .get_item(VERSION_RELOAD_ATTEMPTED_KEY)
                .ok()
                .flatten()
        })
        .is_some_and(|stored_version| stored_version == backend_version)
}

fn set_reload_attempted(backend_version: &str) {
    if let Some(storage) = get_session_storage() {
        let _ = storage.set_item(VERSION_RELOAD_ATTEMPTED_KEY, backend_version);
    }
}

fn clear_reload_attempted() {
    if let Some(storage) = get_session_storage() {
        let _ = storage.remove_item(VERSION_RELOAD_ATTEMPTED_KEY);
    }
}

fn trigger_cache_busting_reload(backend_version: &str) {
    let result = (|| -> Result<(), anyhow::Error> {
        let window = web_sys::window().context("Unable to get the window object")?;
        let location = window.location();
        let href = location
            .href()
            .map_err(|err| JsError::try_from(err).unwrap())
            .context("Unable to get current href")?;

        // Build a cache-busting URL by adding/replacing the _v query parameter
        let mut url = url::Url::parse(&href).context("Unable to parse current URL")?;
        // Remove any existing _v parameter
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| k != "_v")
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        url.query_pairs_mut().clear();
        for (k, v) in pairs {
            url.query_pairs_mut().append_pair(&k, &v);
        }
        url.query_pairs_mut().append_pair("_v", backend_version);

        debug!("Triggering cache-busting reload to: {url}");

        // Use location.replace() to avoid adding to browser history
        location
            .replace(url.as_str())
            .map_err(|err| JsError::try_from(err).unwrap())
            .context("Unable to trigger reload")?;

        Ok(())
    })();

    if let Err(err) = result {
        warn!("Failed to trigger cache-busting reload: {err:?}");
        // Fall back to setting the mismatch signal so the user sees the warning
        *VERSION_MISMATCH.write() = Some(backend_version.to_string());
    }
}
