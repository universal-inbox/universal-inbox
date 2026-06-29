use playwright_rs::Viewport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedState {
    /// Default test user: every integration connected, seeded with sample data.
    Default,
    /// Logged-out browser context — used for /login, /signup pages.
    LoggedOut,
    /// GitHub connection: status=Validated, registered_oauth_scopes=[] (drives
    /// the "missing OAuth scopes" UI state).
    GithubMissingScopes,
    /// GitHub connection: status=Created (disconnected). Mutation order matters —
    /// this is applied last because it invalidates other GitHub states.
    GithubDisconnected,
}

impl SeedState {
    /// Application order: any state listed BEFORE another may run first. This is
    /// the order the orchestrator processes specs in. Choose an order such that
    /// destructive mutations (disconnect) come last.
    pub const APPLY_ORDER: &'static [SeedState] = &[
        SeedState::Default,
        SeedState::LoggedOut,
        SeedState::GithubMissingScopes,
        SeedState::GithubDisconnected,
    ];
}

#[derive(Debug, Clone)]
pub enum Capture {
    /// Capture the visible viewport.
    Viewport,
    /// Capture the full scrollable page.
    FullPage,
    /// Capture a single element matched by the CSS selector.
    Element(&'static str),
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Click the first element matching the selector.
    Click(&'static str),
    /// Wait until an element matching the selector is visible.
    WaitFor(&'static str),
    /// Hover over the first element matching the selector.
    Hover(&'static str),
    /// Sleep for a fixed number of milliseconds (use sparingly).
    Sleep(u64),
    /// Evaluate a JavaScript expression in the page context.
    Evaluate(&'static str),
}

#[derive(Debug, Clone)]
pub struct ScreenshotSpec {
    /// Short identifier for `--only` filtering (without the .png extension).
    pub name: &'static str,
    /// Destination path relative to the doc src root, e.g. `quick_start/images/inbox-screen.png`.
    pub dest: &'static str,
    /// Path to navigate to (relative to the web base URL). Use `None` to stay on the current page.
    pub path: Option<&'static str>,
    /// Per-spec viewport override.
    pub viewport: Viewport,
    /// Required seed state (drives which user is logged in and how the DB is seeded).
    pub state: SeedState,
    /// Setup actions performed after navigation, before the capture.
    pub pre: &'static [Action],
    /// What to capture.
    pub capture: Capture,
}

const PORTRAIT_NARROW: Viewport = Viewport {
    width: 750,
    height: 1320,
};

const WIDE: Viewport = Viewport {
    width: 1280,
    height: 800,
};

// ---- Selector library ----
//
// All selectors below are valid against the redesigned UI. Notes:
//
// - Integration cards: each is a `.integration-card` whose header carries an
//   aria-label like "Toggle Github settings". Use `:has(...)` to target a
//   specific provider's card without coupling to text or order.
// - Notification rows: `.ui-nrow`. Each row contains a provider icon span
//   whose class includes the iconify identifier (e.g. `logos--github-icon`).
//   Combined with `:has([class*='...'])` this lets us pick a row by source.
// - Preview pane: `.detail-panel` (only present when a notification is
//   selected — clicking a row makes it appear).
// - Action buttons (delete/snooze/unsubscribe/…): plain `<button>` with an
//   `aria-label` set from the action title.
// - Modals: stable ids `#task-planning-modal`, `#task-linking-modal`.
// - Sync LED dots in the expanded integration card body: `.sync-led.{ok,error,pending,active}`.
const LINEAR_ROW: &str = ".ui-nrow:has([class*='logos--linear-icon'])";
const GCAL_ROW: &str = ".ui-nrow:has([class*='logos--google-calendar'])";
const ANY_ROW: &str = "#notifications-list .ui-nrow";

// Integration card selectors anchor on the stable `#integration-card-{Kind}` id
// emitted by the integration `Card` (the suffix is the `IntegrationProviderKind`
// variant name), so they match the card regardless of whether it's connected
// (header is a clickable toggle) or disconnected (header is a "Connect"
// call-to-action). The header selectors reuse the aria-label set on the toggle
// button by `IntegrationSettings` — only meaningful when the connection is
// Validated/Failing.
const GITHUB_CARD: &str = "#integration-card-Github";
const GITHUB_CARD_HEADER: &str = "[aria-label=\"Toggle Github settings\"]";
const LINEAR_CARD: &str = "#integration-card-Linear";
const LINEAR_CARD_HEADER: &str = "[aria-label=\"Toggle Linear settings\"]";
const SLACK_CARD: &str = "#integration-card-Slack";
const SLACK_CARD_HEADER: &str = "[aria-label=\"Toggle Slack settings\"]";
const GMAIL_CARD: &str = "#integration-card-GoogleMail";
const GMAIL_CARD_HEADER: &str = "[aria-label=\"Toggle Google Mail settings\"]";
const GCAL_CARD: &str = "#integration-card-GoogleCalendar";
const GCAL_CARD_HEADER: &str = "[aria-label=\"Toggle Google Calendar settings\"]";
const GDRIVE_CARD: &str = "#integration-card-GoogleDrive";
const GDRIVE_CARD_HEADER: &str = "[aria-label=\"Toggle Google Drive settings\"]";
const TODOIST_CARD: &str = "#integration-card-Todoist";
const TODOIST_CARD_HEADER: &str = "[aria-label=\"Toggle Todoist settings\"]";
const TICKTICK_CARD: &str = "#integration-card-TickTick";
const TICKTICK_CARD_HEADER: &str = "[aria-label=\"Toggle Tick Tick settings\"]";

/// The full screenshot manifest used by `cargo run -- test generate-doc-screenshots`.
pub const MANIFEST: &[ScreenshotSpec] = &[
    // ---------- quick_start/images ----------
    // Crop the auth pages to the `.auth-canvas` card so the surrounding
    // viewport whitespace is dropped from the doc embed.
    ScreenshotSpec {
        name: "login-page",
        dest: "quick_start/images/login-page.png",
        path: Some("/login"),
        viewport: PORTRAIT_NARROW,
        state: SeedState::LoggedOut,
        pre: &[
            Action::WaitFor("input[name='email']"),
            Action::WaitFor(".auth-canvas"),
            Action::Sleep(200),
        ],
        capture: Capture::Element(".auth-canvas"),
    },
    ScreenshotSpec {
        name: "signup-page",
        dest: "quick_start/images/signup-page.png",
        path: Some("/signup"),
        viewport: PORTRAIT_NARROW,
        state: SeedState::LoggedOut,
        pre: &[
            Action::WaitFor("input[name='email']"),
            Action::WaitFor(".auth-canvas"),
            Action::Sleep(200),
        ],
        capture: Capture::Element(".auth-canvas"),
    },
    ScreenshotSpec {
        name: "passkey-signup-page",
        dest: "quick_start/images/passkey-signup-page.png",
        path: Some("/passkey-signup"),
        viewport: PORTRAIT_NARROW,
        state: SeedState::LoggedOut,
        pre: &[
            Action::WaitFor("input[name='username']"),
            Action::WaitFor(".auth-canvas"),
            Action::Sleep(200),
        ],
        capture: Capture::Element(".auth-canvas"),
    },
    ScreenshotSpec {
        name: "inbox-screen",
        dest: "quick_start/images/inbox-screen.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[Action::WaitFor(ANY_ROW), Action::Sleep(500)],
        capture: Capture::Viewport,
    },
    ScreenshotSpec {
        name: "synced-tasks-screen",
        dest: "quick_start/images/synced-tasks-screen.png",
        path: Some("/synced-tasks"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[Action::WaitFor("#tasks-page"), Action::Sleep(500)],
        capture: Capture::Viewport,
    },
    ScreenshotSpec {
        name: "first-start-settings-screen",
        dest: "quick_start/images/first-start-settings-screen.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[Action::WaitFor(".settings-container"), Action::Sleep(500)],
        capture: Capture::Viewport,
    },
    ScreenshotSpec {
        name: "linear-issue-preview",
        dest: "quick_start/images/linear-issue-preview.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(LINEAR_ROW),
            Action::Click(LINEAR_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(500),
        ],
        capture: Capture::Element("#detail-panel"),
    },
    // ---------- config/setup/images: integration cards (expanded) ----------
    ScreenshotSpec {
        name: "github-config",
        dest: "config/setup/images/github-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(GITHUB_CARD),
            Action::Click(GITHUB_CARD_HEADER),
            Action::Sleep(500),
        ],
        capture: Capture::Element(GITHUB_CARD),
    },
    ScreenshotSpec {
        name: "linear-config",
        dest: "config/setup/images/linear-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(LINEAR_CARD),
            Action::Click(LINEAR_CARD_HEADER),
            Action::Sleep(500),
        ],
        capture: Capture::Element(LINEAR_CARD),
    },
    // Slack card has 3 sub-tabs (Reaction / Mention / Extension) — clip the
    // expanded card once per tab, clicking the correct segmented-choice button
    // in between. The buttons render as `<button role="tab">` with the label
    // as text content (see web/src/components/integrations/slack/config.rs and
    // web/src/components/settings_controls.rs::SegmentedChoice).
    ScreenshotSpec {
        name: "slack-reaction-config",
        dest: "config/setup/images/slack-reaction-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(SLACK_CARD),
            Action::Click(SLACK_CARD_HEADER),
            Action::Sleep(300),
            Action::Click("button[role='tab']:has-text(\"Reaction\")"),
            Action::Sleep(200),
        ],
        capture: Capture::Element(SLACK_CARD),
    },
    ScreenshotSpec {
        name: "slack-mention-config",
        dest: "config/setup/images/slack-mention-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(SLACK_CARD),
            Action::Click(SLACK_CARD_HEADER),
            Action::Sleep(300),
            Action::Click("button[role='tab']:has-text(\"Mention\")"),
            Action::Sleep(200),
        ],
        capture: Capture::Element(SLACK_CARD),
    },
    ScreenshotSpec {
        name: "slack-extension-config",
        dest: "config/setup/images/slack-extension-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(SLACK_CARD),
            Action::Click(SLACK_CARD_HEADER),
            Action::Sleep(300),
            Action::Click("button[role='tab']:has-text(\"Extension\")"),
            Action::Sleep(200),
        ],
        capture: Capture::Element(SLACK_CARD),
    },
    ScreenshotSpec {
        name: "gmail-config",
        dest: "config/setup/images/google-mail-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(GMAIL_CARD),
            Action::Click(GMAIL_CARD_HEADER),
            Action::Sleep(500),
        ],
        capture: Capture::Element(GMAIL_CARD),
    },
    ScreenshotSpec {
        name: "google-calendar-config",
        dest: "config/setup/images/google-calendar-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(GCAL_CARD),
            Action::Click(GCAL_CARD_HEADER),
            Action::Sleep(500),
        ],
        capture: Capture::Element(GCAL_CARD),
    },
    ScreenshotSpec {
        name: "google-drive-config",
        dest: "config/setup/images/google-drive-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(GDRIVE_CARD),
            Action::Click(GDRIVE_CARD_HEADER),
            Action::Sleep(500),
        ],
        capture: Capture::Element(GDRIVE_CARD),
    },
    ScreenshotSpec {
        name: "todoist-config",
        dest: "config/setup/images/todoist-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(TODOIST_CARD),
            Action::Click(TODOIST_CARD_HEADER),
            Action::Sleep(500),
        ],
        capture: Capture::Element(TODOIST_CARD),
    },
    ScreenshotSpec {
        name: "ticktick-config",
        dest: "config/setup/images/ticktick-config.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(TICKTICK_CARD),
            Action::Click(TICKTICK_CARD_HEADER),
            Action::Sleep(500),
        ],
        capture: Capture::Element(TICKTICK_CARD),
    },
    // ---------- misc/images ----------
    ScreenshotSpec {
        name: "user-profile",
        dest: "misc/images/user-profile.png",
        path: Some("/profile"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[Action::WaitFor(".profile-container"), Action::Sleep(300)],
        capture: Capture::Viewport,
    },
    // /security page has two cards rendered as `<section role="region" aria-label=...>`
    // — clip each section individually for the AI-agents and API-usage docs.
    // The seed in `screenshots::generate_doc_screenshots` populates two API
    // tokens and one authorized OAuth client so the cards have content.
    ScreenshotSpec {
        name: "ai-agents-security",
        dest: "misc/images/ai_agents.png",
        path: Some("/security"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor("section[aria-label='Authorized OAuth2 clients']"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("section[aria-label='Authorized OAuth2 clients']"),
    },
    ScreenshotSpec {
        name: "api-keys-security",
        dest: "misc/images/api_usage.png",
        path: Some("/security"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor("section[aria-label='API keys']"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("section[aria-label='API keys']"),
    },
    // Dedicated screenshot for the new Security & Privacy doc (misc/security.md).
    // Same `/security` page section as `ai-agents-security`, but written to a
    // distinct destination so the Security doc can evolve independently.
    ScreenshotSpec {
        name: "security-oauth-clients",
        dest: "misc/images/security-oauth-clients.png",
        path: Some("/security"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor("section[aria-label='Authorized OAuth2 clients']"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("section[aria-label='Authorized OAuth2 clients']"),
    },
    // The Authentication methods card lives on the /profile page (not /security)
    // — it's part of the user profile rather than the security overview.
    ScreenshotSpec {
        name: "security-auth-methods",
        dest: "misc/images/security-auth-methods.png",
        path: Some("/profile"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor("section[aria-label='Authentication methods']"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("section[aria-label='Authentication methods']"),
    },
    // ---------- Preview-pane action button clips ----------
    // Each captures a tiny icon button from the preview pane after clicking a
    // notification. The action-button aria-labels are stable ("Delete
    // notification", "Snooze notification", etc., per `get_notification_action_buttons`).
    ScreenshotSpec {
        name: "delete-button",
        dest: "quick_start/images/delete-button.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(ANY_ROW),
            Action::Click(ANY_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("button[aria-label='Delete notification']"),
    },
    ScreenshotSpec {
        name: "snooze-button",
        dest: "quick_start/images/snooze-button.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(ANY_ROW),
            Action::Click(ANY_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("button[aria-label='Snooze notification']"),
    },
    ScreenshotSpec {
        name: "unsubscribe-button",
        dest: "quick_start/images/unsubscribe-button.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(ANY_ROW),
            Action::Click(ANY_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("button[aria-label='Unsubscribe from the notification']"),
    },
    ScreenshotSpec {
        name: "create-task-button",
        dest: "quick_start/images/create-task-button.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(ANY_ROW),
            Action::Click(ANY_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("button[aria-label='Create task']"),
    },
    ScreenshotSpec {
        name: "create-task-with-defaults-button",
        dest: "quick_start/images/create-task-with-defaults-button.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(ANY_ROW),
            Action::Click(ANY_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("button[aria-label='Create task with defaults']"),
    },
    ScreenshotSpec {
        name: "link-to-task-button",
        dest: "quick_start/images/link-to-task-button.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(ANY_ROW),
            Action::Click(ANY_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(300),
        ],
        capture: Capture::Element("button[aria-label='Link to task']"),
    },
    // ---------- Modals ----------
    ScreenshotSpec {
        name: "create-task-modal",
        dest: "quick_start/images/create-task-modal.png",
        path: Some("/"),
        viewport: PORTRAIT_NARROW,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(ANY_ROW),
            Action::Click(ANY_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Click("button[aria-label='Create task']"),
            Action::WaitFor("#task-planning-modal"),
            Action::Sleep(500),
        ],
        capture: Capture::Element("#task-planning-modal .modal-content"),
    },
    ScreenshotSpec {
        name: "link-to-task-modal",
        dest: "quick_start/images/link-to-task-modal.png",
        path: Some("/"),
        viewport: PORTRAIT_NARROW,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(ANY_ROW),
            Action::Click(ANY_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Click("button[aria-label='Link to task']"),
            Action::WaitFor("#task-linking-modal"),
            Action::Sleep(500),
        ],
        capture: Capture::Element("#task-linking-modal .modal-content"),
    },
    // ---------- Google Calendar action buttons ----------
    // The Google Calendar event preview renders a row of yes/no/maybe RSVP
    // buttons inside `.preview-rsvp-inline`. We clip just that row so the doc
    // embed can sit on a single line next to its explanation (see inbox_screen.md
    // — the image is sized `=x30`, so anything larger than the button strip
    // would render as an unreadable squashed block).
    ScreenshotSpec {
        name: "google-calendar-action-buttons",
        dest: "quick_start/images/google-calendar-action-buttons.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(GCAL_ROW),
            Action::Click(GCAL_ROW),
            Action::WaitFor("#detail-panel"),
            Action::WaitFor(".preview-rsvp-inline"),
            Action::Sleep(500),
        ],
        capture: Capture::Element(".preview-rsvp-inline"),
    },
    // RSVP buttons have no aria-label; they're identified by the iconify class
    // on their inner span (user-check / user-x / user-minus).
    ScreenshotSpec {
        name: "yes-button",
        dest: "quick_start/images/yes-button.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(GCAL_ROW),
            Action::Click(GCAL_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(500),
        ],
        capture: Capture::Element(
            ".preview-rsvp-inline button:has(.icon-\\[lucide--user-check\\])",
        ),
    },
    ScreenshotSpec {
        name: "no-button",
        dest: "quick_start/images/no-button.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(GCAL_ROW),
            Action::Click(GCAL_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(500),
        ],
        capture: Capture::Element(".preview-rsvp-inline button:has(.icon-\\[lucide--user-x\\])"),
    },
    ScreenshotSpec {
        name: "maybe-button",
        dest: "quick_start/images/maybe-button.png",
        path: Some("/"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[
            Action::WaitFor(GCAL_ROW),
            Action::Click(GCAL_ROW),
            Action::WaitFor("#detail-panel"),
            Action::Sleep(500),
        ],
        capture: Capture::Element(
            ".preview-rsvp-inline button:has(.icon-\\[lucide--user-minus\\])",
        ),
    },
    // ---------- Task priority indicators ----------
    // The redesigned tasks page no longer uses bookmark icons — priority is
    // shown via colored Lucide circles in the row's meta icon slot (Todoist),
    // or a circle-dot for Linear issues (which carry their own workflow state).
    // We clip the meta icon span for one row per priority level.
    // Linear tasks render a neutral circle-dot meta icon (no priority color).
    ScreenshotSpec {
        name: "task-bookmark-gray",
        dest: "quick_start/images/task-bookmark-gray.png",
        path: Some("/synced-tasks"),
        viewport: WIDE,
        state: SeedState::Default,
        pre: &[Action::WaitFor("#tasks-page"), Action::Sleep(500)],
        capture: Capture::Element(
            "#tasks-page .ui-nrow:has(.icon-\\[lucide--circle-dot\\]) .ui-nrow-meta-icon",
        ),
    },
    // The remaining task-bookmark-{yellow,orange,red} variants from the old UI
    // are listed under MANUAL: the test user only seeds two visible synced
    // tasks (Linear + Slack reaction) — there's no Todoist task in the
    // priority palette to clip. Maintainers can update those PNGs by hand once
    // the seed includes a Todoist task per priority level.
    // ---------- GitHub error states ----------
    ScreenshotSpec {
        name: "github-missing-oauth-scopes",
        dest: "config/setup/images/github-missing-oauth-scopes.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::GithubMissingScopes,
        pre: &[Action::WaitFor(GITHUB_CARD), Action::Sleep(500)],
        capture: Capture::Element(GITHUB_CARD),
    },
    ScreenshotSpec {
        name: "github-disconnected",
        dest: "config/setup/images/github-disconnected.png",
        path: Some("/settings"),
        viewport: WIDE,
        state: SeedState::GithubDisconnected,
        pre: &[Action::WaitFor(GITHUB_CARD), Action::Sleep(500)],
        capture: Capture::Element(GITHUB_CARD),
    },
];

/// Screenshots that cannot be automated and remain manual.
pub const MANUAL: &[(&str, &str)] = &[
    (
        "raycast-extension",
        "misc/images/raycast-extension.png — Raycast extension UI (external app)",
    ),
    (
        "raycast-setup",
        "misc/images/raycast-setup.png — Raycast setup screen (external app)",
    ),
    (
        "task-bookmark-yellow",
        "quick_start/images/task-bookmark-yellow.png — needs a Todoist task with priority Normal (text-yellow-500) in the seed",
    ),
    (
        "task-bookmark-orange",
        "quick_start/images/task-bookmark-orange.png — needs a Todoist task with priority High (text-orange-500) in the seed",
    ),
    (
        "task-bookmark-red",
        "quick_start/images/task-bookmark-red.png — needs a Todoist task with priority Urgent (text-red-500) in the seed",
    ),
];
