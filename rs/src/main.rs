use anyhow::{Result, anyhow};
use clap::{ArgAction, Parser};
use colored::*;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::SystemTime;
use tokio::runtime::Runtime;

mod config;
mod test;
mod test_step;

use crate::config::ConfigData;
use crate::test::Test;
use crate::test::TestResult;
use crate::test::print_test_results;

fn is_yaml(path: &PathBuf) -> bool {
    if let Some(extension) = path.extension() {
        return extension == "yaml" || extension == "yml";
    }
    false
}

fn is_test_file(path: &PathBuf) -> bool {
    if !is_yaml(path) {
        return false;
    }
    if let Some(stem) = path
        .file_stem()
        .and_then(|v| v.to_str())
        .map(|v| v.to_lowercase())
    {
        return stem.starts_with("test") || stem.ends_with("test");
    }
    false
}

fn is_root_dir(path: &PathBuf) -> bool {
    path.is_dir() && path.join(".git").exists()
}

#[derive(Parser, Debug)]
#[command(version, about = "Yapitest is a simple API testing platform")]
struct Args {
    paths: Vec<String>,

    // Test Groups to run
    #[arg(short = 'g', action = ArgAction::Append)]
    group: Vec<String>,

    // Tests to exclude
    #[arg(short = 'x', action = ArgAction::Append)]
    exclude: Vec<String>,

    // Tests to include
    #[arg(short = 'i', action = ArgAction::Append)]
    include: Vec<String>,

    // Number of threads
    #[arg(short = 't')]
    threads: Option<u64>,

    // Output verbosity: 0=silent, 1=names only, 2=default, 3=assertions
    #[arg(short = 'v', default_value_t = 2)]
    verbosity: u8,
}

fn get_config_in_dir(path: &PathBuf) -> Result<Option<ConfigData>> {
    let yapitest_config_names = [
        "yapitest-config.yaml",
        "yapitest-config.yml",
        "config.yaml",
        "config.yml",
    ];
    for config_name in &yapitest_config_names {
        let config_path = path.join(config_name);
        if config_path.exists() {
            return ConfigData::from_file(&config_path)
                .map(Some)
                .map_err(|e| anyhow!("{}", e));
        }
    }
    Ok(None)
}

fn load_tests_from_file(
    configs: &mut HashMap<PathBuf, Arc<RwLock<ConfigData>>>,
    path: &PathBuf,
) -> anyhow::Result<Vec<Test>, anyhow::Error> {
    if !is_test_file(path) {
        return Ok(vec![]);
    }

    let mut deepest_config_key: Option<PathBuf> = None;

    let (cfg_opt, mut tests) = Test::load_from_file(path)?;

    // If a config exists inline in the test file, register it and set it on all tests.
    if let Some(config) = cfg_opt.map(|v| Arc::new(RwLock::new(v))) {
        let config_path = config.read().unwrap().path.clone();
        configs.insert(config_path.clone(), Arc::clone(&config));
        deepest_config_key = Some(config_path);
        for test in tests.iter_mut() {
            test.add_config(Arc::clone(&config));
        }
    }

    for ancestor in path.ancestors() {
        let ancestor_pb = ancestor.to_path_buf();

        let mut ancestor_config: Option<Arc<RwLock<ConfigData>>> = None;

        // Get Ancestor Config if it exists
        if let Some(anc_config) = configs.get(ancestor) {
            ancestor_config = Some(Arc::clone(anc_config));
        } else {
            match get_config_in_dir(&ancestor_pb) {
                Ok(Some(anc_config)) => {
                    let arc_anc_config = Arc::new(RwLock::new(anc_config));
                    configs.insert(ancestor_pb.clone(), Arc::clone(&arc_anc_config));
                    ancestor_config = Some(arc_anc_config);
                }
                Ok(None) => {}
                Err(e) => return Err(anyhow!(e)),
            }
        }

        // Set Ancestor config as parent of tests & configs
        if let Some(anc_config) = ancestor_config {
            if let Some(deepest_config) = deepest_config_key
                .as_ref()
                .and_then(|k| configs.get_mut(k))
                .and_then(|a| Arc::get_mut(a))
            {
                deepest_config
                    .write()
                    .unwrap()
                    .set_parent(Arc::clone(&anc_config));
            }

            for test in tests.iter_mut() {
                test.add_config(Arc::clone(&anc_config));
            }
            deepest_config_key = Some(ancestor_pb);
        }

        // Found root of file system or `.git` directory. Exit.
        if is_root_dir(&ancestor.to_path_buf()) {
            break;
        }
    }

    Ok(tests)
}

fn load_tests_in_dir(
    configs: &mut HashMap<PathBuf, Arc<RwLock<ConfigData>>>,
    path: &PathBuf,
) -> anyhow::Result<Vec<Test>, anyhow::Error> {
    let mut output: Vec<Test> = vec![];

    if let Ok(read_dir) = std::fs::read_dir(path) {
        for item_res in read_dir {
            match item_res {
                Ok(item) => {
                    let item_path = item.path();
                    if item_path.is_dir() {
                        match load_tests_in_dir(configs, &item_path) {
                            Ok(new_tests) => output.extend(new_tests),
                            Err(e) => panic!("{}", e),
                        }
                    } else {
                        match load_tests_from_file(configs, &item_path) {
                            Ok(new_tests) => output.extend(new_tests),
                            Err(e) => panic!("{}", e),
                        }
                    }
                }
                Err(e) => panic!("{}", e),
            }
        }
    }

    Ok(output)
}

fn load_tests(
    configs: &mut HashMap<PathBuf, Arc<RwLock<ConfigData>>>,
    path: &PathBuf,
) -> anyhow::Result<Vec<Test>, anyhow::Error> {
    if path.is_dir() {
        load_tests_in_dir(configs, path)
    } else {
        load_tests_from_file(configs, path)
    }
}

async fn run_tests_thread(tests: &[Test], verbosity: u8) -> Vec<TestResult> {
    let mut output: Vec<TestResult> = Vec::with_capacity(tests.len());
    for test in tests {
        let result = test.run().await;
        if verbosity >= 1 {
            if result.passed() {
                println!("  {}  {}", "PASS".green(), result.name());
            } else {
                println!("  {}  {}", "FAIL".red().bold(), result.name());
            }
            if verbosity >= 3 {
                for assertion in result.assertions() {
                    if assertion.passed {
                        println!("      {}  {}", "✓".green(), assertion.name);
                    } else {
                        println!(
                            "      {}  {}",
                            "✗".red(),
                            assertion.message.as_deref().unwrap_or(&assertion.name)
                        );
                    }
                }
            }
            io::stdout().flush().unwrap();
        }
        output.push(result);
    }
    output
}

async fn run_tests(tests: &[Test], threads: Option<u64>, verbosity: u8) -> Vec<TestResult> {
    let num_threads = threads.unwrap_or_else(|| {
        let available = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        ((available * 3 / 4) as u64).max(1)
    });

    if num_threads == 1 {
        return run_tests_thread(tests, verbosity).await;
    }

    // Group tests by source file before distributing to threads. Tests in the
    // same file share state through config step-sets (e.g. `once: true` groups
    // like `create-user`), so they must run sequentially on the same thread to
    // avoid concurrent mutations of shared API state.
    let mut file_order: Vec<&PathBuf> = Vec::new();
    let mut file_groups: HashMap<&PathBuf, Vec<Test>> = HashMap::new();

    for test in tests {
        let path = test.path();
        match file_groups.entry(path) {
            Entry::Vacant(e) => {
                file_order.push(path);
                e.insert(vec![test.clone()]);
            }
            Entry::Occupied(e) => {
                e.into_mut().push(test.clone());
            }
        }
    }

    // Distribute whole file groups round-robin across threads.
    // Cap thread count at the number of distinct files.
    let actual_threads = (num_threads as usize).min(file_order.len());
    let mut thread_groups: Vec<Vec<Test>> = (0..actual_threads).map(|_| Vec::new()).collect();

    for (i, path) in file_order.into_iter().enumerate() {
        if let Some(group) = file_groups.remove(path) {
            thread_groups[i % actual_threads].extend(group);
        }
    }

    let (tx, rx) = mpsc::channel::<Vec<TestResult>>();

    thread::scope(|s| {
        for group in thread_groups {
            let tx_clone = tx.clone();
            s.spawn(move || {
                let rt = Runtime::new().expect("Failed to create runtime");
                let group_results = rt.block_on(async { run_tests_thread(&group, verbosity).await });
                let _ = tx_clone.send(group_results);
            });
        }
        drop(tx);
    });

    let mut all_results: Vec<TestResult> = Vec::new();
    while let Ok(group_results) = rx.recv() {
        all_results.extend(group_results);
    }

    all_results
}

#[tokio::main]
async fn main() {
    let start_time = SystemTime::now();

    let args = Args::parse();

    let mut test_paths: Vec<PathBuf> = Vec::new();
    for path_arg in &args.paths {
        let path = PathBuf::from(path_arg);
        if path.exists() {
            match std::fs::canonicalize(&path) {
                Ok(p) => test_paths.push(p),
                Err(e) => panic!("Error Unwrapping Path {}: {}", path_arg, e),
            }
        } else {
            panic!("Path \"{}\" does not exist. Exiting.", path_arg)
        }
    }

    let verbosity = args.verbosity;

    if verbosity >= 2 {
        let divider = "─".repeat(40);
        println!("yapitest v{}", env!("CARGO_PKG_VERSION"));
        println!("{}", divider.dimmed());
    }

    let mut configs: HashMap<PathBuf, Arc<RwLock<ConfigData>>> = HashMap::new();
    let mut tests: Vec<Test> = vec![];
    if verbosity >= 2 {
        println!("{}", "Collecting tests...".dimmed());
    }
    for path in &test_paths {
        match load_tests(&mut configs, path) {
            Ok(found_tests) => tests.extend(found_tests),
            Err(e) => panic!("{}", e),
        }
    }

    fn contains_group(test: &Test, groups: &[&String]) -> bool {
        test.groups
            .as_ref()
            .is_some_and(|tg| groups.iter().any(|g| tg.contains(*g)))
    }

    fn contains_text(test: &Test, texts: &[&String]) -> bool {
        texts.iter().any(|t| test.name.contains(t.as_str()))
    }

    if !args.group.is_empty() {
        let groups: Vec<&String> = args.group.iter().collect();
        tests.retain(|t| contains_group(t, &groups));
    }

    if !args.include.is_empty() {
        let includes: Vec<&String> = args.include.iter().collect();
        tests.retain(|t| contains_text(t, &includes));
    }

    if !args.exclude.is_empty() {
        let excludes: Vec<&String> = args.exclude.iter().collect();
        tests.retain(|t| !contains_text(t, &excludes));
    }

    if verbosity >= 2 {
        println!("{}", format!("Found {} tests", tests.len()).dimmed());
        println!();
    }
    let test_results = run_tests(&tests, args.threads, verbosity).await;
    let end_time = SystemTime::now();
    let duration = end_time
        .duration_since(start_time)
        .expect("Time went backwards")
        .as_secs_f32();
    print_test_results(&test_results, duration, verbosity);

    let any_failed = test_results.iter().any(|r| !r.passed());
    std::process::exit(if any_failed { 1 } else { 0 });
}
