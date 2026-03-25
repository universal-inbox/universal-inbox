use std::{path::Path, time::Duration};

use playwright_rs::{
    Browser, BrowserContext, BrowserContextOptions, LaunchOptions, Page, Playwright, RecordVideo,
    Viewport, expect,
};

use crate::commands::generate::DEFAULT_PASSWORD;

pub const EXPECT_TIMEOUT: Duration = Duration::from_secs(60);

pub const DEFAULT_VIEWPORT: Viewport = Viewport {
    width: 1280,
    height: 800,
};

pub async fn launch_browser() -> anyhow::Result<(Playwright, Browser)> {
    let playwright = Playwright::launch()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to launch Playwright: {e}"))?;
    let browser = playwright
        .chromium()
        .launch_with_options(LaunchOptions::default().chromium_sandbox(false))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to launch Chromium: {e}"))?;
    Ok((playwright, browser))
}

pub async fn new_page(
    browser: &Browser,
    viewport: Viewport,
) -> anyhow::Result<(BrowserContext, Page)> {
    new_context(browser, viewport, None).await
}

/// Creates a browser context that records every page to `record_video_dir` as a `.webm`.
///
/// Playwright finalises the video file on context close — callers must call `context.close()`
/// before looking for the output, and the configured directory must exist.
pub async fn new_recording_page(
    browser: &Browser,
    viewport: Viewport,
    record_video_dir: &Path,
) -> anyhow::Result<(BrowserContext, Page)> {
    new_context(browser, viewport, Some(record_video_dir)).await
}

async fn new_context(
    browser: &Browser,
    viewport: Viewport,
    record_video_dir: Option<&Path>,
) -> anyhow::Result<(BrowserContext, Page)> {
    // `device_scale_factor(2.0)` produces retina-quality (2×) screenshots so
    // element clips embedded in the doc don't look blurry when scaled up. Costs
    // ~4× the PNG file size but the docs are read-only.
    let mut builder = BrowserContextOptions::builder()
        .viewport(viewport.clone())
        .device_scale_factor(2.0);
    if let Some(dir) = record_video_dir {
        builder = builder.record_video(RecordVideo {
            dir: dir.to_string_lossy().into_owned(),
            size: Some(viewport),
        });
    }
    let opts = builder.build();
    let context = browser
        .new_context_with_options(opts)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create browser context: {e}"))?;

    // Force light mode before any page script runs. The web frontend persists
    // the user's theme choice in localStorage under `color-theme`, falling back
    // to `prefers-color-scheme: dark` when absent — headless Chromium often
    // follows the host OS, so docs would land in dark mode without this prime.
    // Also set the `data-theme` attribute directly so the very first paint
    // (before WASM mounts) doesn't flash dark.
    context
        .add_init_script(LIGHT_MODE_INIT_SCRIPT)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to add light-mode init script: {e}"))?;

    let page = context
        .new_page()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create page: {e}"))?;

    let _ = page
        .route("**/*headwayapp.co*", |route| async move {
            route.abort(None).await
        })
        .await;
    let _ = page
        .route("**/*cdn.*", |route| async move { route.abort(None).await })
        .await;

    Ok((context, page))
}

const LIGHT_MODE_INIT_SCRIPT: &str = r#"
try {
  localStorage.setItem('color-theme', 'light');
  document.documentElement.setAttribute('data-theme', 'corporate');
  document.documentElement.classList.remove('dark');
} catch (e) {}
"#;

pub async fn login(page: &Page, base_url: &str, email: &str) -> anyhow::Result<()> {
    page.goto(&format!("{base_url}/login"), None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to navigate to login page: {e}"))?;

    let email_input = page.locator("input[name='email']").await;
    expect(email_input.clone())
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .map_err(|e| anyhow::anyhow!("Email input not visible on login page: {e}"))?;
    email_input
        .fill(email, None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fill email: {e}"))?;

    let password_input = page.locator("input[name='password']").await;
    password_input
        .fill(DEFAULT_PASSWORD, None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fill password: {e}"))?;

    let submit = page.locator("button[type='submit']").await;
    submit
        .click(None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to click submit: {e}"))?;

    let notifications_page = page.locator("#notifications-page").await;
    expect(notifications_page)
        .with_timeout(EXPECT_TIMEOUT)
        .to_be_visible()
        .await
        .map_err(|e| anyhow::anyhow!("Notifications page not visible after login: {e}"))?;

    Ok(())
}
