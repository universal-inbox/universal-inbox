//! `cargo run --features screenshots -- test generate-doc-screenshots`
//!
//! Drives a headless Chromium against the locally running Universal Inbox web frontend
//! to regenerate every screenshot embedded in the mdBook documentation. Designed to be
//! invoked by `just doc update-screenshots` after the API + web servers are up.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use playwright_rs::{Page, ScreenshotOptions, ScreenshotType, expect};
use tokio::sync::RwLock;
use tracing::{info, warn};
use universal_inbox::user::UserId;

use crate::{
    commands::{generate, user},
    configuration::Settings,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, user::service::UserService,
    },
};

pub mod browser;
pub mod manifest;
pub mod screencast;
pub mod states;

use browser::{EXPECT_TIMEOUT, launch_browser, login, new_page};
use manifest::{Action, Capture, MANIFEST, MANUAL, ScreenshotSpec, SeedState};

#[tracing::instrument(
    name = "generate-doc-screenshots",
    level = "info",
    skip(
        user_service,
        integration_connection_service,
        notification_service,
        task_service,
        third_party_item_service,
        settings
    ),
    err
)]
#[allow(clippy::too_many_arguments)]
pub async fn generate_doc_screenshots(
    user_service: Arc<UserService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: Settings,
    base_url: String,
    output_dir: PathBuf,
    only: Option<Vec<String>>,
    keep_user: bool,
) -> Result<(), UniversalInboxError> {
    if !output_dir.is_dir() {
        return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
            "--output-dir does not exist or is not a directory: {}",
            output_dir.display()
        )));
    }

    let selected: Vec<&ScreenshotSpec> = if let Some(filter) = &only {
        MANIFEST
            .iter()
            .filter(|spec| filter.iter().any(|f| f == spec.name))
            .collect()
    } else {
        MANIFEST.iter().collect()
    };

    if selected.is_empty() {
        warn!("No screenshots selected; check the --only filter against the manifest.");
        return Ok(());
    }

    info!(
        "Generating {} screenshot(s) from base_url={base_url}, output_dir={}",
        selected.len(),
        output_dir.display()
    );

    info!("Generating test user with seed data…");
    let email = generate::generate_testing_user(
        user_service.clone(),
        integration_connection_service.clone(),
        notification_service,
        task_service,
        third_party_item_service,
        settings,
    )
    .await
    .context("Failed to generate test user")?;
    info!("Test user generated: {email}");

    let user_id = lookup_user_id(&user_service, &email)
        .await
        .context("Failed to look up freshly-generated test user")?;

    // Seed extras the default fixture doesn't produce: API tokens + authorized
    // OAuth clients so the /security page has content for ai_agents.md and
    // api_usage.md. Logged-as-warn on failure rather than aborting the run.
    if let Err(err) = states::seed_security_artifacts(user_service.clone(), user_id).await {
        warn!("Failed to seed security-page artifacts: {err:#}");
    }

    let cleanup_outcome = run_with_browser(
        &base_url,
        &email,
        user_id,
        integration_connection_service,
        &selected,
        &output_dir,
    )
    .await;

    if !keep_user {
        info!("Deleting test user {user_id}");
        if let Err(err) = user::delete_user(user_service, user_id).await {
            warn!("Failed to delete test user {user_id}: {err:?}");
        }
    } else {
        warn!("--keep-user set: leaving test user {email} in the database");
    }

    cleanup_outcome?;

    if !MANUAL.is_empty() {
        info!("Skipped (manual):");
        for (_name, note) in MANUAL {
            info!("  • {note}");
        }
    }

    Ok(())
}

async fn lookup_user_id(
    user_service: &Arc<UserService>,
    email: &str,
) -> Result<UserId, UniversalInboxError> {
    let parsed: email_address::EmailAddress = email
        .parse()
        .with_context(|| format!("Invalid email returned by generate_testing_user: {email}"))?;

    let mut tx = user_service
        .begin()
        .await
        .context("Failed to open transaction while resolving test user id")?;
    let user = user_service
        .get_user_by_email(&mut tx, &parsed)
        .await?
        .ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow::anyhow!(
                "Could not find freshly-created test user by email {email}"
            ))
        })?;
    tx.rollback().await.ok();
    Ok(user.id)
}

async fn run_with_browser(
    base_url: &str,
    email: &str,
    user_id: UserId,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    specs: &[&ScreenshotSpec],
    output_dir: &Path,
) -> Result<(), UniversalInboxError> {
    info!("Launching headless Chromium…");
    let (_playwright, browser) = launch_browser()
        .await
        .map_err(UniversalInboxError::Unexpected)?;

    let mut successes = 0usize;
    let mut failures: Vec<(String, String)> = Vec::new();

    for state in SeedState::APPLY_ORDER {
        let group: Vec<&&ScreenshotSpec> =
            specs.iter().filter(|spec| spec.state == *state).collect();
        if group.is_empty() {
            continue;
        }

        let group_result = run_state_group(
            &browser,
            base_url,
            email,
            *state,
            user_id,
            integration_connection_service.clone(),
            &group,
            output_dir,
        )
        .await;
        for (spec, result) in group.iter().zip(group_result) {
            match result {
                Ok(()) => {
                    successes += 1;
                    info!("  ✓ wrote {}", spec.dest);
                }
                Err(msg) => {
                    warn!("  ✗ {}: {}", spec.name, msg);
                    failures.push((spec.name.to_string(), msg));
                }
            }
        }
    }

    info!(
        "Done: {successes}/{} captured, {} failed.",
        specs.len(),
        failures.len()
    );

    if !failures.is_empty() {
        for (name, msg) in &failures {
            warn!("  ✗ {name}: {msg}");
        }
        return Err(UniversalInboxError::Unexpected(anyhow::anyhow!(
            "{} screenshot(s) failed; see log above",
            failures.len()
        )));
    }

    Ok(())
}

/// Run every spec in a state group inside a single browser context.
///
/// Logs in once (when the state requires authentication) and then per-spec
/// navigates to the target path. Avoids re-downloading the ~74 MB debug WASM
/// bundle for every spec, which is the main reason individual specs would
/// time out at the 60s assertion deadline.
#[allow(clippy::too_many_arguments)]
async fn run_state_group(
    browser: &playwright_rs::Browser,
    base_url: &str,
    email: &str,
    state: SeedState,
    user_id: UserId,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    specs: &[&&ScreenshotSpec],
    output_dir: &Path,
) -> Vec<Result<(), String>> {
    let mut results = Vec::with_capacity(specs.len());

    if specs.is_empty() {
        return results;
    }

    // Pick the widest viewport in the group as the context default. Per-spec
    // overrides happen via set_viewport_size below.
    let context_viewport = specs
        .iter()
        .map(|s| (s.viewport.width, s.viewport.height))
        .max()
        .map(|(w, h)| playwright_rs::Viewport {
            width: w,
            height: h,
        })
        .unwrap_or(browser::DEFAULT_VIEWPORT);

    let (context, page) = match new_page(browser, context_viewport).await {
        Ok(ok) => ok,
        Err(err) => {
            let msg = format!("Failed to open browser context for state {state:?}: {err:#}");
            for _ in specs {
                results.push(Err(msg.clone()));
            }
            return results;
        }
    };

    let needs_login = !matches!(state, SeedState::LoggedOut);
    if needs_login {
        info!("[{state:?}] logging in…");
        if let Err(err) = login(&page, base_url, email).await {
            let msg = format!("login failed for state {state:?}: {err:#}");
            for _ in specs {
                results.push(Err(msg.clone()));
            }
            let _ = context.close().await;
            return results;
        }
    }

    // Apply DB state mutations AFTER login because the notifications page load
    // triggers `trigger_sync_for_integration_connections`, which would clobber
    // any sync-state mutation made before login.
    if let Err(err) = apply_state(state, user_id, integration_connection_service.clone()).await {
        let msg = format!("failed to apply DB state {state:?}: {err:#}");
        warn!("  ✗ {msg}");
        for _ in specs {
            results.push(Err(msg.clone()));
        }
        let _ = context.close().await;
        return results;
    }

    for spec in specs {
        info!("▶ [{state:?}] {} → {}", spec.name, spec.dest);
        let result = run_single_spec(&page, base_url, spec, output_dir).await;
        results.push(result.map_err(|e| format!("{e:#}")));
    }

    let _ = context.close().await;
    results
}

async fn run_single_spec(
    page: &Page,
    base_url: &str,
    spec: &ScreenshotSpec,
    output_dir: &Path,
) -> anyhow::Result<()> {
    page.set_viewport_size(spec.viewport.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to set viewport: {e}"))?;

    if let Some(path) = spec.path {
        page.goto(&format!("{base_url}{path}"), None)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to navigate to {path}: {e}"))?;
    }

    for action in spec.pre {
        run_action(page, action).await?;
    }

    let dest_path = output_dir.join(spec.dest);
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create dir {}", parent.display()))?;
    }

    capture(page, &spec.capture, &dest_path).await?;
    Ok(())
}

async fn apply_state(
    state: SeedState,
    user_id: UserId,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
) -> anyhow::Result<()> {
    match state {
        SeedState::Default | SeedState::LoggedOut => Ok(()),
        SeedState::GithubMissingScopes => {
            states::set_github_missing_scopes(integration_connection_service, user_id).await
        }
        SeedState::GithubDisconnected => {
            states::set_github_disconnected(integration_connection_service, user_id).await
        }
    }
}

async fn run_action(page: &Page, action: &Action) -> anyhow::Result<()> {
    match action {
        Action::Click(selector) => {
            let loc = page.locator(selector).await;
            loc.first()
                .click(None)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to click `{selector}`: {e}"))?;
        }
        Action::WaitFor(selector) => {
            let loc = page.locator(selector).await;
            expect(loc.first())
                .with_timeout(EXPECT_TIMEOUT)
                .to_be_visible()
                .await
                .map_err(|e| anyhow::anyhow!("WaitFor `{selector}` timed out: {e}"))?;
        }
        Action::Hover(selector) => {
            let loc = page.locator(selector).await;
            loc.first()
                .hover(None)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to hover `{selector}`: {e}"))?;
        }
        Action::Sleep(ms) => {
            tokio::time::sleep(Duration::from_millis(*ms)).await;
        }
        Action::Evaluate(expr) => {
            page.evaluate_expression(expr)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to evaluate `{expr}`: {e}"))?;
        }
    }
    Ok(())
}

async fn capture(page: &Page, capture: &Capture, dest: &std::path::Path) -> anyhow::Result<()> {
    let opts = ScreenshotOptions::builder()
        .screenshot_type(ScreenshotType::Png)
        .full_page(matches!(capture, Capture::FullPage))
        .build();

    match capture {
        Capture::Viewport | Capture::FullPage => {
            page.screenshot_to_file(dest, Some(opts))
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Failed to capture screenshot to {}: {e}", dest.display())
                })?;
        }
        Capture::Element(selector) => {
            let loc = page.locator(selector).await;
            let bytes = loc
                .first()
                .screenshot(Some(
                    ScreenshotOptions::builder()
                        .screenshot_type(ScreenshotType::Png)
                        .build(),
                ))
                .await
                .map_err(|e| anyhow::anyhow!("Failed to capture element `{selector}`: {e}"))?;
            tokio::fs::write(dest, &bytes)
                .await
                .with_context(|| format!("Failed to write {}", dest.display()))?;
        }
    }
    Ok(())
}
