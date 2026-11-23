//! Runs the Gherkin scenarios of `tests/features` as individual tests.
//!
//! Every expanded scenario (one per Scenario, one per Examples row of a Scenario Outline) is a
//! libtest trial, so nextest lists them and runs each one alone in its own process: scenarios
//! never share the process-wide state of the app under test (cache namespace, tracing, rustls).

use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Debug},
    fs,
    path::{Path, PathBuf},
    sync::Once,
};

use cucumber::{
    World, cli, codegen::WorldInventory, event, feature::Ext as _, gherkin, writer::Stats as _,
};
use futures::future::LocalBoxFuture;
use libtest_mimic::{Arguments, Failed, Trial};

use universal_inbox_api::configuration::Settings;

use crate::common::{init_tracing, settings};

pub const FEATURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/features");

/// Prints `<legacy tag>\t<scenario>` lines instead of running anything, for the legacy mapping
/// check (`just api check-legacy-mapping`).
const LEGACY_REPORT_ENV: &str = "UI_SCENARIOS_LEGACY_REPORT";
const LEGACY_TAG_PREFIX: &str = "legacy:";

pub trait ScenarioWorld: World + Debug + WorldInventory {
    /// Stops what the scenario started, once its steps ran.
    fn tear_down(&mut self) -> LocalBoxFuture<'_, ()>;
}

/// How the app under test is started. Given steps that configure the app change it before the
/// app starts; they compose, whatever their order.
pub struct AppConfig {
    pub settings: Settings,
    pub billing: bool,
    pub sign_in: SignInMode,
}

/// How the users of the app can sign in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SignInMode {
    /// Single sign-on with an identity provider.
    #[default]
    SingleSignOn,
    /// Email and password, or a passkey. There is no identity provider.
    PasswordAndPasskey,
    /// Email and password, with no passkey. There is no identity provider.
    PasswordOnly,
    /// Single sign-on, email and password, or a passkey.
    SingleSignOnPasswordAndPasskey,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            settings: settings(),
            billing: false,
            sign_in: SignInMode::default(),
        }
    }
}

/// An app a scenario can start from an [`AppConfig`] and stop.
pub trait ScenarioApp: Sized {
    fn start(config: AppConfig) -> LocalBoxFuture<'static, Self>;
    fn stop(self) -> LocalBoxFuture<'static, ()>;
}

/// The app under test of a scenario, started on first use.
pub struct AppUnderTest<A> {
    config: Option<AppConfig>,
    app: Option<A>,
}

impl<A> Default for AppUnderTest<A> {
    fn default() -> Self {
        Self {
            config: Some(AppConfig::default()),
            app: None,
        }
    }
}

impl<A> Debug for AppUnderTest<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = if self.app.is_some() {
            "started"
        } else {
            "not started"
        };
        f.write_str(state)
    }
}

impl<A: ScenarioApp> AppUnderTest<A> {
    /// The configuration of the app, for a step that configures it.
    pub fn configure(&mut self) -> &mut AppConfig {
        self.config.as_mut().expect(
            "This step configures the app: it must come before the first step that uses the app",
        )
    }

    pub async fn get(&mut self) -> &A {
        if let Some(config) = self.config.take() {
            self.app = Some(A::start(config).await);
        }
        self.app
            .as_ref()
            .expect("The app could not be started earlier in this scenario")
    }

    pub async fn stop(&mut self) {
        if let Some(app) = self.app.take() {
            app.stop().await;
        }
    }
}

pub struct Scenario {
    pub name: String,
    pub path: PathBuf,
    pub line: usize,
    /// Name of the expanded scenario, as cucumber knows it.
    pub scenario_name: String,
    pub legacy_tags: Vec<String>,
}

/// Lists every expanded scenario of the feature files under [`FEATURES_DIR`] whose path,
/// relative to it, matches `include`.
pub fn collect(include: fn(&Path) -> bool) -> Vec<Scenario> {
    let root = Path::new(FEATURES_DIR);
    let mut paths = feature_files(root);
    paths.retain(|path| include(path.strip_prefix(root).unwrap()));
    paths.sort();

    let mut scenarios: Vec<Scenario> = paths
        .iter()
        .flat_map(|path| feature_scenarios(root, path))
        .collect();

    let mut occurrences: HashMap<String, usize> = HashMap::new();
    for scenario in &scenarios {
        *occurrences.entry(scenario.name.clone()).or_default() += 1;
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    for scenario in &mut scenarios {
        if occurrences[&scenario.name] > 1 {
            let index = seen.entry(scenario.name.clone()).or_default();
            *index += 1;
            scenario.name = format!("{} #{index}", scenario.name);
        }
    }
    scenarios
}

fn feature_files(dir: &Path) -> Vec<PathBuf> {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("Cannot read the features directory {dir:?}: {err}"));
    entries
        .flat_map(|entry| {
            let path = entry
                .expect("Cannot read a features directory entry")
                .path();
            if path.is_dir() {
                feature_files(&path)
            } else if path.extension().is_some_and(|ext| ext == "feature") {
                vec![path]
            } else {
                vec![]
            }
        })
        .collect()
}

fn feature_scenarios(root: &Path, path: &Path) -> Vec<Scenario> {
    let feature = gherkin::Feature::parse_path(path, gherkin::GherkinEnv::default())
        .unwrap_or_else(|err| panic!("Cannot parse {path:?}: {err}"))
        .expand_examples()
        .unwrap_or_else(|err| panic!("Cannot expand the examples of {path:?}: {err}"));
    let prefix = path
        .strip_prefix(root)
        .unwrap()
        .with_extension("")
        .to_string_lossy()
        .into_owned();

    let in_rules = feature.rules.iter().flat_map(|rule| {
        rule.scenarios
            .iter()
            .map(move |scenario| (Some(rule), scenario))
    });
    let scenarios: Vec<Scenario> = feature
        .scenarios
        .iter()
        .map(|scenario| (None, scenario))
        .chain(in_rules)
        .map(|(rule, scenario)| {
            let name = match rule {
                Some(rule) => format!("{prefix}::{}::{}", rule.name, scenario.name),
                None => format!("{prefix}::{}", scenario.name),
            };
            let legacy_tags = feature
                .tags
                .iter()
                .chain(rule.iter().flat_map(|rule| &rule.tags))
                .chain(&scenario.tags)
                .filter_map(|tag| {
                    tag.trim_start_matches('@')
                        .strip_prefix(LEGACY_TAG_PREFIX)
                        .map(str::to_string)
                })
                .collect();
            Scenario {
                name,
                path: path.to_path_buf(),
                line: scenario.position.line,
                scenario_name: scenario.name.clone(),
                legacy_tags,
            }
        })
        .collect();

    let mut lines = HashSet::new();
    for scenario in &scenarios {
        assert!(
            lines.insert(scenario.line),
            "Two scenarios of {path:?} expand to line {}: a scenario must be found by its line",
            scenario.line
        );
    }
    scenarios
}

/// Entry point of a scenarios test binary.
pub fn main<W: ScenarioWorld>(include: fn(&Path) -> bool) {
    let scenarios = collect(include);

    if std::env::var_os(LEGACY_REPORT_ENV).is_some() {
        for scenario in &scenarios {
            for tag in &scenario.legacy_tags {
                println!("{tag}\t{}", scenario.name);
            }
        }
        return;
    }

    let mut args = Arguments::from_args();
    // Run by hand (`cargo test`), the trials of one process still run one at a time.
    args.test_threads = Some(1);
    let trials = scenarios
        .into_iter()
        .map(|scenario| Trial::test(scenario.name.clone(), move || run::<W>(&scenario)))
        .collect();
    libtest_mimic::run(&args, trials).exit();
}

fn run<W: ScenarioWorld>(scenario: &Scenario) -> Result<(), Failed> {
    static TRACING: Once = Once::new();
    TRACING.call_once(|| init_tracing(&settings()));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("Cannot build the scenario runtime: {err}"))?;
    let line = scenario.line;
    let scenario_name = scenario.scenario_name.clone();
    let writer = runtime.block_on(
        W::cucumber()
            .with_cli(cli::Opts::<_, _, _, cli::Empty>::default())
            .fail_on_skipped()
            .after(|_, _, _, _: &event::ScenarioFinished, world| {
                Box::pin(async move {
                    if let Some(world) = world {
                        world.tear_down().await;
                    }
                })
            })
            .filter_run(scenario.path.clone(), move |_, _, candidate| {
                candidate.position.line == line && candidate.name == scenario_name
            }),
    );

    let stats = writer.scenarios_stats();
    if writer.execution_has_failed() || stats.failed > 0 {
        return Err("The scenario failed".into());
    }
    if stats.passed != 1 {
        return Err(format!(
            "Expected the scenario at {}:{line} to run and pass once, got {stats:?}",
            scenario.path.display()
        )
        .into());
    }
    Ok(())
}
