//! `cargo run --features screenshots -- test record-landing-screencast --output ./screen.webm`
//!
//! Drives a headless Chromium through the landing-page demo scenario at deterministic,
//! millisecond-accurate timings and records the whole session to a `.webm` file. Voiceover and
//! avatar PiP composition are layered on top by a second pass (HeyGen + ffmpeg) driven by an AI
//! agent — see `web/screencasts/landing-page.md`.
//!
//! Why this lives next to `screenshots/`: the still-screenshot pipeline already owns the
//! `playwright-rs` dependency (gated by the `screenshots` cargo feature), the browser launch
//! helper, and the login helper. Forking a parallel module would duplicate that plumbing.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, anyhow};
use playwright_rs::{Page, Viewport, expect};
use serde::Serialize;
use tokio::{sync::RwLock, time::sleep};
use tracing::{info, warn};
use universal_inbox::{integration_connection::provider::IntegrationProviderKind, user::UserId};
use uuid::Uuid;

use crate::{
    commands::{
        generate::{self, DEFAULT_PASSWORD},
        screenshots::browser::{launch_browser, new_recording_page},
        user,
    },
    configuration::Settings,
    universal_inbox::{
        UniversalInboxError, integration_connection::service::IntegrationConnectionService,
        notification::service::NotificationService, task::service::TaskService,
        third_party::service::ThirdPartyItemService, user::service::UserService,
    },
};

// ---------- Recording-specific viewport ----------
//
// 720p capture — smaller viewport renders UI text relatively larger so the
// final composite is readable for the viewer. The browser still uses
// device_scale_factor=2 (retina rendering), so the captured frames are
// effectively 2× pixel density. Composer scales up to 1920×1080 in post.
const RECORDING_VIEWPORT: Viewport = Viewport {
    width: 1280,
    height: 720,
};

// ---------- Selectors (kept close to the scenario for easy editing) ----------

const GITHUB_CARD: &str = ".integration-card:has(:text-is(\"Github\"))";
const GITHUB_CARD_HEADER: &str = "[aria-label=\"Toggle Github settings\"]";
const ANY_NOTIFICATION_ROW: &str = "#notifications-list .ui-nrow";
const SLACK_THREAD_ROW: &str = "#notifications-list .ui-nrow[data-provider='slack']";
const GITHUB_PR_ROW: &str =
    "#notifications-list .ui-nrow[data-provider='github'][data-kind='pull_request']";
const GCAL_EVENT_ROW: &str = "#notifications-list .ui-nrow[data-provider='google_calendar']";
const DISCONNECTED_INTEGRATION_CARD: &str = ".integration-card.disconnected-card";
const DETAIL_PANEL: &str = ".detail-panel";
const TASK_PLANNING_MODAL: &str = "#task-planning-modal";
const TASK_PLANNING_MODAL_CANCEL: &str = "#task-planning-modal button:has-text(\"Cancel\")";
// ui-redesign removed the legacy `.settings-container` class; we now wait for any
// integration card to be visible (an unambiguous signal that the settings page has
// rendered) since `.integration-card` is emitted by the Card UI component.
const SETTINGS_CONTAINER: &str = ".integration-card";
const TASKS_PAGE: &str = "#tasks-page";
const DELETE_ALL_BUTTON: &str = "button[aria-label='Delete all notifications']";
const DELETE_ALL_MODAL: &str = "#delete-all-confirmation-modal";
const DELETE_ALL_CONFIRM: &str = "#delete-all-confirmation-modal button:has-text('Delete all')";

/// One row in the sidecar `<output>.beats.json` file.
/// Lets the compositing agent cut the recorded `.webm` at exact beat boundaries
/// rather than estimating from cumulative `Sleep`/`WaitFor` durations.
#[derive(Serialize)]
struct BeatTimestamp {
    name: String,
    start_ms: u128,
}

#[derive(Clone, Copy)]
struct ScenarioBeat {
    name: &'static str,
    actions: &'static [BeatAction],
}

#[derive(Clone, Copy)]
enum BeatAction {
    /// Navigate to `${base_url}{path}`.
    Goto(&'static str),
    /// Wait until at least one element matching `selector` is visible (60 s timeout).
    WaitFor(&'static str),
    /// Wait until no element matching `selector` is visible.
    WaitForGone(&'static str),
    /// `tokio::time::sleep` between actions — the cadence-control primitive.
    Sleep(u64),
    /// Hover the first element matching `selector`.
    Hover(&'static str),
    /// Click the first element matching `selector`.
    Click(&'static str),
    /// Press a key on the focused page (e.g. `"d"`, `"ArrowDown"`).
    Press(&'static str),
    /// Reload the current page.
    Reload,
    /// Mark the given integration as Validated for the recording user (no real OAuth).
    ConnectIntegration(IntegrationProviderKind),
    /// Run the full notifications/tasks seed for the recording user.
    GenerateNotifications,
}

/// The landing-page screencast — beats encoded in execution order.
/// Voiceover + captions for each beat live in `web/screencasts/landing-page.md` (the editorial
/// source of truth); this list is the *execution* source of truth.
const SCENARIO: &[ScenarioBeat] = &[
    ScenarioBeat {
        name: "0 - hook (login + arrive on inbox)",
        actions: &[
            // Login is special-cased — the runner drives it BEFORE this beat fires (see
            // `slow_login` in run_recording). That helper navigates to /login, holds on the
            // empty form, fills email + password with visible pauses between fields, holds on
            // the filled form, then submits and waits for #notifications-page. By the time
            // this beat starts, the empty inbox is on screen.
            BeatAction::Sleep(1_000),
        ],
    },
    ScenarioBeat {
        name: "1 - empty state",
        actions: &[BeatAction::Sleep(8_000)],
    },
    ScenarioBeat {
        name: "2 - connect github (no real oauth)",
        actions: &[
            BeatAction::Goto("/settings"),
            BeatAction::WaitFor(SETTINGS_CONTAINER),
            BeatAction::WaitFor(GITHUB_CARD),
            BeatAction::Hover(GITHUB_CARD),
            BeatAction::Sleep(1_500),
            BeatAction::ConnectIntegration(IntegrationProviderKind::Github),
            BeatAction::Sleep(300),
            BeatAction::Reload,
            BeatAction::WaitFor(GITHUB_CARD_HEADER),
            BeatAction::Sleep(2_000),
        ],
    },
    ScenarioBeat {
        name: "2b - all integrations connected (settings)",
        actions: &[
            // Seed all integrations + notifications/tasks. The settings page is still in front
            // of the camera; after a Reload it will reflect every card as Connected.
            BeatAction::GenerateNotifications,
            BeatAction::Reload,
            BeatAction::WaitFor(SETTINGS_CONTAINER),
            BeatAction::WaitForGone(DISCONNECTED_INTEGRATION_CARD),
            BeatAction::Sleep(8_000),
        ],
    },
    ScenarioBeat {
        name: "3 - populate inbox",
        actions: &[
            BeatAction::Goto("/"),
            BeatAction::WaitFor(ANY_NOTIFICATION_ROW),
            BeatAction::Sleep(1_000),
        ],
    },
    ScenarioBeat {
        name: "4 - rich preview (slack -> github pr -> google calendar)",
        actions: &[
            // ~3 s intro hold on first preview while Jake says "Each notification opens with
            // full context", then click through Slack → GH PR → Calendar at ~2 s intervals to
            // match Jake's rapid enumeration of the three notification types.
            BeatAction::Click(SLACK_THREAD_ROW),
            BeatAction::WaitFor(DETAIL_PANEL),
            BeatAction::Sleep(3_000),
            BeatAction::Click(GITHUB_PR_ROW),
            BeatAction::Sleep(2_000),
            BeatAction::Click(GCAL_EVENT_ROW),
            BeatAction::Sleep(3_000),
        ],
    },
    ScenarioBeat {
        name: "5 - keyboard triage + plan into task manager",
        actions: &[
            // Reload first so AuthenticatedApp re-mounts and re-fetches integration
            // connections. is_task_actions_enabled (the gate for the P keyboard shortcut's
            // planning modal) is set when that fetch completes; SPA navigation alone doesn't
            // refresh it, so without this the P press is a silent no-op.
            BeatAction::Reload,
            BeatAction::WaitFor(ANY_NOTIFICATION_ROW),
            BeatAction::Sleep(1_200),
            BeatAction::Click(ANY_NOTIFICATION_ROW),
            BeatAction::Sleep(400),
            // Tight d/s/p sequence — a longer pre-d Sleep was found to silently break the
            // modal open. The composer's per-beat trim re-times this fast sequence so the
            // visible key-press toasts and modal appearance line up with Jake's slower
            // spoken cues ("Press d to dismiss, or s to snooze... press p to plan").
            BeatAction::Press("d"),
            BeatAction::Sleep(700),
            BeatAction::Press("s"),
            BeatAction::Sleep(1_500),
            BeatAction::Press("p"),
            BeatAction::WaitFor(TASK_PLANNING_MODAL),
            BeatAction::Sleep(800), // fade-in animation settle
            BeatAction::Sleep(4_000),
            // Close modal via Cancel button — flyonui-modal's `close_flyonui_modal` call is
            // wired to this button (Esc keypress alone is unreliable across the Dioxus port).
            // The selector targets the unique "Cancel" button inside the planning modal.
            BeatAction::Click(TASK_PLANNING_MODAL_CANCEL),
            BeatAction::WaitForGone(TASK_PLANNING_MODAL),
            BeatAction::Sleep(500),
        ],
    },
    ScenarioBeat {
        name: "6 - tasks page",
        actions: &[
            BeatAction::Goto("/synced-tasks"),
            BeatAction::WaitFor(TASKS_PAGE),
            BeatAction::Sleep(5_000),
        ],
    },
    ScenarioBeat {
        name: "7 - inbox zero (delete all)",
        actions: &[
            BeatAction::Goto("/"),
            BeatAction::WaitFor(ANY_NOTIFICATION_ROW),
            BeatAction::Sleep(800),
            BeatAction::Hover(DELETE_ALL_BUTTON),
            BeatAction::Sleep(600),
            BeatAction::Click(DELETE_ALL_BUTTON),
            BeatAction::WaitFor(DELETE_ALL_MODAL),
            BeatAction::Sleep(1_500),
            BeatAction::Click(DELETE_ALL_CONFIRM),
            BeatAction::WaitForGone(ANY_NOTIFICATION_ROW),
            BeatAction::Sleep(2_000),
        ],
    },
    ScenarioBeat {
        name: "8 - call to action",
        actions: &[BeatAction::Sleep(5_000)],
    },
];

#[tracing::instrument(
    name = "record-landing-screencast",
    level = "info",
    skip(
        user_service,
        integration_connection_service,
        notification_service,
        task_service,
        third_party_item_service,
        settings,
    ),
    err
)]
#[allow(clippy::too_many_arguments)]
pub async fn record_landing_screencast(
    user_service: Arc<UserService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: Settings,
    base_url: String,
    output: PathBuf,
    keep_user: bool,
) -> Result<(), UniversalInboxError> {
    info!("Generating fresh empty user for the recording…");
    let email = generate::generate_empty_user(user_service.clone())
        .await
        .context("Failed to generate empty user")?;
    info!("Empty user generated: {email}");

    let user_id = lookup_user_id(&user_service, &email)
        .await
        .context("Failed to look up freshly-generated empty user")?;

    let recording_dir =
        std::env::temp_dir().join(format!("universal-inbox-screencast-{}", Uuid::new_v4()));
    tokio::fs::create_dir_all(&recording_dir)
        .await
        .with_context(|| format!("Failed to create recording dir {}", recording_dir.display()))?;

    let outcome = run_recording(
        &base_url,
        &email,
        user_id,
        user_service.clone(),
        integration_connection_service,
        notification_service,
        task_service,
        third_party_item_service,
        settings,
        &recording_dir,
        &output,
    )
    .await;

    // Best-effort cleanup of the temp recording dir (the .webm has been moved out already).
    let _ = tokio::fs::remove_dir_all(&recording_dir).await;

    if !keep_user {
        info!("Deleting recording user {user_id}");
        if let Err(err) = user::delete_user(user_service, user_id).await {
            warn!("Failed to delete recording user {user_id}: {err:?}");
        }
    } else {
        warn!("--keep-user set: leaving recording user {email} in the database");
    }

    outcome
}

#[allow(clippy::too_many_arguments)]
async fn run_recording(
    base_url: &str,
    email: &str,
    user_id: UserId,
    user_service: Arc<UserService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: Settings,
    recording_dir: &std::path::Path,
    output: &std::path::Path,
) -> Result<(), UniversalInboxError> {
    info!(
        "Launching headless Chromium with video recording → {} at {}x{}",
        recording_dir.display(),
        RECORDING_VIEWPORT.width,
        RECORDING_VIEWPORT.height
    );
    let (_playwright, browser) = launch_browser()
        .await
        .map_err(UniversalInboxError::Unexpected)?;

    let (context, page) = new_recording_page(&browser, RECORDING_VIEWPORT, recording_dir)
        .await
        .map_err(UniversalInboxError::Unexpected)?;

    // Beat 0 prologue — visible login. The bare login() helper completes in ~0.5 s which is
    // too fast for the login screen to register as a beat; slow_login drives the same flow but
    // holds visibly on the empty form, between fields, and on the filled form before submit.
    let mut beat_timestamps: Vec<BeatTimestamp> = Vec::with_capacity(SCENARIO.len() + 3);
    let recording_start = Instant::now();
    beat_timestamps.push(BeatTimestamp {
        name: "_login_prologue".to_string(),
        start_ms: 0,
    });
    info!("[beat 0 prologue] visible login as {email}");
    if let Err(err) = slow_login(&page, base_url, email).await {
        let _ = context.close().await;
        return Err(UniversalInboxError::Unexpected(anyhow!(
            "login failed during beat 0: {err:#}"
        )));
    }

    for beat in SCENARIO {
        let t_ms = recording_start.elapsed().as_millis();
        beat_timestamps.push(BeatTimestamp {
            name: beat.name.to_string(),
            start_ms: t_ms,
        });
        info!("▶ beat {} (t={}ms)", beat.name, t_ms);
        for action in beat.actions {
            if let Err(err) = run_action(
                &page,
                base_url,
                user_id,
                action,
                user_service.clone(),
                integration_connection_service.clone(),
                notification_service.clone(),
                task_service.clone(),
                third_party_item_service.clone(),
                settings.clone(),
            )
            .await
            {
                let _ = context.close().await;
                return Err(UniversalInboxError::Unexpected(anyhow!(
                    "beat `{}` failed: {err:#}",
                    beat.name
                )));
            }
        }
    }

    beat_timestamps.push(BeatTimestamp {
        name: "_recording_end".to_string(),
        start_ms: recording_start.elapsed().as_millis(),
    });

    // Close the context to flush the .webm file.
    info!("Closing browser context to flush video recording…");
    context
        .close()
        .await
        .map_err(|e| UniversalInboxError::Unexpected(anyhow!("Failed to close context: {e}")))?;

    // Locate the single .webm in the recording dir and move it to `output`.
    let mut entries = tokio::fs::read_dir(recording_dir)
        .await
        .context("Failed to list video recording directory")?;
    let mut webm: Option<std::path::PathBuf> = None;
    while let Some(entry) = entries
        .next_entry()
        .await
        .context("Failed to read recording dir entry")?
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("webm") {
            if webm.is_some() {
                return Err(UniversalInboxError::Unexpected(anyhow!(
                    "Recording directory contains more than one .webm file; refusing to guess which one to keep"
                )));
            }
            webm = Some(path);
        }
    }
    let webm = webm.ok_or_else(|| {
        UniversalInboxError::Unexpected(anyhow!(
            "No .webm file produced in {} — did playwright-rs honor record_video?",
            recording_dir.display()
        ))
    })?;

    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create output dir {}", parent.display()))?;
    }
    if output.exists() {
        tokio::fs::remove_file(output)
            .await
            .with_context(|| format!("Failed to overwrite existing {}", output.display()))?;
    }
    tokio::fs::rename(&webm, output)
        .await
        .with_context(|| format!("Failed to move {} → {}", webm.display(), output.display()))?;

    let beats_path = beats_sidecar_path(output);
    let beats_json = serde_json::to_string_pretty(&beat_timestamps)
        .context("Failed to serialize beat timestamps")?;
    tokio::fs::write(&beats_path, beats_json)
        .await
        .with_context(|| {
            format!(
                "Failed to write beat timestamps to {}",
                beats_path.display()
            )
        })?;

    info!("Recording written to {}", output.display());
    info!("Beat timestamps written to {}", beats_path.display());
    Ok(())
}

/// `screen.webm` → `screen.beats.json` (alongside the recording).
/// Replaces `with_extension("beats.json")` because that helper inconsistently
/// handles the `.beats.json` (multi-segment) extension across stdlib versions.
fn beats_sidecar_path(output: &Path) -> PathBuf {
    let parent = output.parent().unwrap_or_else(|| Path::new(""));
    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("recording");
    parent.join(format!("{stem}.beats.json"))
}

#[allow(clippy::too_many_arguments)]
async fn run_action(
    page: &Page,
    base_url: &str,
    user_id: UserId,
    action: &BeatAction,
    user_service: Arc<UserService>,
    integration_connection_service: Arc<RwLock<IntegrationConnectionService>>,
    notification_service: Arc<RwLock<NotificationService>>,
    task_service: Arc<RwLock<TaskService>>,
    third_party_item_service: Arc<RwLock<ThirdPartyItemService>>,
    settings: Settings,
) -> anyhow::Result<()> {
    match action {
        BeatAction::Goto(path) => {
            page.goto(&format!("{base_url}{path}"), None)
                .await
                .map_err(|e| anyhow!("Failed to navigate to `{path}`: {e}"))?;
        }
        BeatAction::WaitFor(selector) => {
            // Tolerant: the recording's value is the timed visual cadence, not strict
            // selector assertions. If a selector drifts (UI rename, route slow, etc.) we
            // warn and continue rather than abort the whole take.
            let loc = page.locator(selector).await;
            if let Err(e) = expect(loc.first())
                .with_timeout(Duration::from_secs(10))
                .to_be_visible()
                .await
            {
                warn!("WaitFor `{selector}` timed out (continuing): {e}");
            }
        }
        BeatAction::WaitForGone(selector) => {
            let loc = page.locator(selector).await;
            if let Err(e) = expect(loc.first())
                .with_timeout(Duration::from_secs(10))
                .to_be_hidden()
                .await
            {
                warn!("WaitForGone `{selector}` timed out (continuing): {e}");
            }
        }
        BeatAction::Sleep(ms) => {
            sleep(Duration::from_millis(*ms)).await;
        }
        BeatAction::Hover(selector) => {
            let loc = page.locator(selector).await;
            loc.first()
                .hover(None)
                .await
                .map_err(|e| anyhow!("Failed to hover `{selector}`: {e}"))?;
        }
        BeatAction::Click(selector) => {
            let loc = page.locator(selector).await;
            loc.first()
                .click(None)
                .await
                .map_err(|e| anyhow!("Failed to click `{selector}`: {e}"))?;
        }
        BeatAction::Press(key) => {
            page.keyboard()
                .press(key, None)
                .await
                .map_err(|e| anyhow!("Failed to press `{key}`: {e}"))?;
        }
        BeatAction::Reload => {
            page.reload(None)
                .await
                .map_err(|e| anyhow!("Failed to reload page: {e}"))?;
        }
        BeatAction::ConnectIntegration(kind) => {
            generate::connect_integration_for_user(
                user_service,
                integration_connection_service,
                settings,
                user_id,
                *kind,
            )
            .await
            .context("ConnectIntegration beat action failed")?;
        }
        BeatAction::GenerateNotifications => {
            generate::generate_notifications_for_user(
                user_service,
                integration_connection_service,
                notification_service,
                task_service,
                third_party_item_service,
                settings,
                user_id,
                vec![],
            )
            .await
            .context("GenerateNotifications beat action failed")?;
        }
    }
    Ok(())
}

/// Visible-pacing wrapper around the bare login flow. Drives /login → fill email → fill
/// password → submit, but with deliberate `sleep`s between each step so the login form
/// registers as its own beat in the recording (the unembellished `screenshots::browser::login`
/// completes in ~0.5 s — invisible in the final video).
async fn slow_login(page: &Page, base_url: &str, email: &str) -> anyhow::Result<()> {
    page.goto(&format!("{base_url}/login"), None)
        .await
        .map_err(|e| anyhow!("Failed to navigate to login page: {e}"))?;

    let email_input = page.locator("input[name='email']").await;
    expect(email_input.clone())
        .with_timeout(Duration::from_secs(30))
        .to_be_visible()
        .await
        .map_err(|e| anyhow!("Email input not visible on login page: {e}"))?;

    // Hold on the empty login form so the viewer sees we're starting from a fresh app surface.
    sleep(Duration::from_millis(1_500)).await;

    email_input
        .fill(email, None)
        .await
        .map_err(|e| anyhow!("Failed to fill email: {e}"))?;
    sleep(Duration::from_millis(600)).await;

    let password_input = page.locator("input[name='password']").await;
    password_input
        .fill(DEFAULT_PASSWORD, None)
        .await
        .map_err(|e| anyhow!("Failed to fill password: {e}"))?;
    sleep(Duration::from_millis(700)).await;

    let submit = page.locator("button[type='submit']").await;
    submit
        .click(None)
        .await
        .map_err(|e| anyhow!("Failed to click submit: {e}"))?;

    let notifications_page = page.locator("#notifications-page").await;
    expect(notifications_page)
        .with_timeout(Duration::from_secs(30))
        .to_be_visible()
        .await
        .map_err(|e| anyhow!("Notifications page not visible after login: {e}"))?;

    Ok(())
}

async fn lookup_user_id(
    user_service: &Arc<UserService>,
    email: &str,
) -> Result<UserId, UniversalInboxError> {
    let parsed: email_address::EmailAddress = email
        .parse()
        .with_context(|| format!("Invalid email returned by generate_empty_user: {email}"))?;

    let mut tx = user_service
        .begin()
        .await
        .context("Failed to open transaction while resolving recording user id")?;
    let user = user_service
        .get_user_by_email(&mut tx, &parsed)
        .await?
        .ok_or_else(|| {
            UniversalInboxError::Unexpected(anyhow!(
                "Could not find freshly-created recording user by email {email}"
            ))
        })?;
    tx.rollback().await.ok();
    Ok(user.id)
}
