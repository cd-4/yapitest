pub mod config;
pub mod test;
pub mod test_step;

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::SystemTime;
use tokio::runtime::Runtime;

pub use config::ConfigData;
pub use test::{Test, TestResult, print_test_results};
pub use test_step::{AssertionResult, TestStepFailureReason, TestStepResult};

pub fn is_yaml(path: &PathBuf) -> bool {
    if let Some(extension) = path.extension() {
        return extension == "yaml" || extension == "yml";
    }
    false
}

pub fn is_test_file(path: &PathBuf) -> bool {
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

pub fn load_tests_from_file(
    configs: &mut HashMap<PathBuf, Arc<RwLock<ConfigData>>>,
    path: &PathBuf,
) -> Result<Vec<Test>> {
    if !is_test_file(path) {
        return Ok(vec![]);
    }

    let mut deepest_config_key: Option<PathBuf> = None;

    let (cfg_opt, mut tests) = Test::load_from_file(path)?;

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

        if is_root_dir(&ancestor.to_path_buf()) {
            break;
        }
    }

    Ok(tests)
}

fn load_tests_in_dir(
    configs: &mut HashMap<PathBuf, Arc<RwLock<ConfigData>>>,
    path: &PathBuf,
) -> Result<Vec<Test>> {
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

pub fn load_tests(
    configs: &mut HashMap<PathBuf, Arc<RwLock<ConfigData>>>,
    path: &PathBuf,
) -> Result<Vec<Test>> {
    if path.is_dir() {
        load_tests_in_dir(configs, path)
    } else {
        load_tests_from_file(configs, path)
    }
}

pub async fn run_tests_thread(tests: &[Test], verbosity: u8) -> Vec<TestResult> {
    let mut output: Vec<TestResult> = Vec::with_capacity(tests.len());
    for test in tests {
        let test_start = SystemTime::now();
        let mut result = test.run().await;
        result.duration_ms = SystemTime::now()
            .duration_since(test_start)
            .unwrap_or_default()
            .as_millis() as u64;
        if verbosity >= 1 {
            use colored::Colorize;
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

pub async fn run_tests(tests: &[Test], threads: Option<u64>, verbosity: u8) -> Vec<TestResult> {
    let num_threads = threads.unwrap_or_else(|| {
        let available = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        ((available * 3 / 4) as u64).max(1)
    });

    if num_threads == 1 {
        return run_tests_thread(tests, verbosity).await;
    }

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

    let actual_threads = (num_threads as usize).min(file_order.len());
    let mut thread_groups: Vec<Vec<Test>> = (0..actual_threads).map(|_| Vec::new()).collect();

    for (i, path) in file_order.into_iter().enumerate() {
        if let Some(group) = file_groups.remove(path) {
            thread_groups[i % actual_threads].extend(group);
        }
    }

    let (tx, rx) = std::sync::mpsc::channel::<Vec<TestResult>>();

    thread::scope(|s| {
        for group in thread_groups {
            let tx_clone = tx.clone();
            s.spawn(move || {
                let rt = Runtime::new().expect("Failed to create runtime");
                let group_results =
                    rt.block_on(async { run_tests_thread(&group, verbosity).await });
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

/// Run tests defined as a parsed YAML value, with an optional config YAML value.
///
/// `tests_yaml` is the parsed content of a test YAML file.
/// `config_yaml` is an optional parsed config to attach to all tests.
pub async fn run_from_yaml(
    tests_yaml: serde_yaml::Value,
    config_yaml: Option<serde_yaml::Value>,
) -> Result<Vec<TestResult>> {
    let virtual_path = PathBuf::from("/yapitest-virtual/test.yaml");
    let (inline_config, mut tests) = Test::load_from_value(tests_yaml, &virtual_path)?;

    let mut config: Option<Arc<RwLock<ConfigData>>> = None;

    if let Some(cfg_val) = config_yaml {
        let cfg = ConfigData::from_val(
            cfg_val,
            &PathBuf::from("/yapitest-virtual/config.yaml"),
        )?;
        config = Some(Arc::new(RwLock::new(cfg)));
    }

    if let Some(inline_cfg) = inline_config {
        let arc_inline = Arc::new(RwLock::new(inline_cfg));
        if let Some(outer) = config {
            arc_inline.write().unwrap().set_parent(outer);
        }
        config = Some(arc_inline);
    }

    if let Some(cfg) = config {
        for test in &mut tests {
            test.add_config(Arc::clone(&cfg));
        }
    }

    Ok(run_tests(&tests, None, 0).await)
}

/// Load and run all tests found at `path`. No console output.
pub async fn run_path(path: &Path) -> Result<Vec<TestResult>> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| anyhow!("{}", e))?;
    let mut configs = HashMap::new();
    let tests = load_tests(&mut configs, &canonical)?;
    Ok(run_tests(&tests, None, 0).await)
}

/// Blocking version of [`run_path`] for use outside of an async context.
pub fn run_path_blocking(path: &Path) -> Result<Vec<TestResult>> {
    let rt = Runtime::new().map_err(|e| anyhow!("{}", e))?;
    rt.block_on(run_path(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_yaml_yaml_extension() {
        assert!(is_yaml(&PathBuf::from("test.yaml")));
    }

    #[test]
    fn test_is_yaml_yml_extension() {
        assert!(is_yaml(&PathBuf::from("test.yml")));
    }

    #[test]
    fn test_is_yaml_json_extension() {
        assert!(!is_yaml(&PathBuf::from("test.json")));
    }

    #[test]
    fn test_is_yaml_no_extension() {
        assert!(!is_yaml(&PathBuf::from("testfile")));
    }

    #[test]
    fn test_is_test_file_test_prefix_yaml() {
        assert!(is_test_file(&PathBuf::from("test-login.yaml")));
    }

    #[test]
    fn test_is_test_file_test_prefix_yml() {
        assert!(is_test_file(&PathBuf::from("test-login.yml")));
    }

    #[test]
    fn test_is_test_file_test_suffix() {
        assert!(is_test_file(&PathBuf::from("login-test.yaml")));
    }

    #[test]
    fn test_is_test_file_config_yaml_excluded() {
        assert!(!is_test_file(&PathBuf::from("config.yaml")));
    }

    #[test]
    fn test_is_test_file_yapitest_config_excluded() {
        assert!(!is_test_file(&PathBuf::from("yapitest-config.yaml")));
    }

    #[test]
    fn test_is_test_file_non_yaml_excluded() {
        assert!(!is_test_file(&PathBuf::from("test-something.json")));
    }

    #[test]
    fn test_is_test_file_case_insensitive_stem() {
        assert!(is_test_file(&PathBuf::from("TEST-login.yaml")));
        assert!(is_test_file(&PathBuf::from("login-TEST.yaml")));
    }
}
