//! Runs the Gherkin scenarios of `tests/features` as individual tests.
//!
//! Every expanded scenario (one per Scenario, one per Examples row of a Scenario Outline) is a
//! libtest trial, so nextest lists them and runs each one alone in its own process: scenarios
//! never share the process-wide state of the app under test (cache namespace, tracing, rustls).

use std::{
    collections::HashMap,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

use cucumber::{
    World, cli, codegen::WorldInventory, event, feature::Ext as _, gherkin, writer::Stats as _,
};
use futures::future::LocalBoxFuture;
use libtest_mimic::{Arguments, Failed, Trial};

pub const FEATURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/features");

/// Prints `<legacy tag>\t<scenario>` lines instead of running anything, for the legacy mapping
/// check (`just api check-legacy-mapping`).
const LEGACY_REPORT_ENV: &str = "UI_SCENARIOS_LEGACY_REPORT";
const LEGACY_TAG_PREFIX: &str = "legacy:";

pub trait ScenarioWorld: World + Debug + WorldInventory {
    fn set_up(&mut self) -> LocalBoxFuture<'_, ()>;
    fn tear_down(&mut self) -> LocalBoxFuture<'_, ()>;
}

pub struct Scenario {
    pub name: String,
    pub path: PathBuf,
    pub line: usize,
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
    feature
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
                legacy_tags,
            }
        })
        .collect()
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
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("Cannot build the scenario runtime: {err}"))?;
    let line = scenario.line;
    let writer = runtime.block_on(
        W::cucumber()
            .with_cli(cli::Opts::<_, _, _, cli::Empty>::default())
            .fail_on_skipped()
            .before(|_, _, _, world| world.set_up())
            .after(|_, _, _, _: &event::ScenarioFinished, world| {
                Box::pin(async move {
                    if let Some(world) = world {
                        world.tear_down().await;
                    }
                })
            })
            .filter_run(scenario.path.clone(), move |_, _, candidate| {
                candidate.position.line == line
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
