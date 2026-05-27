use anyhow::{Error, Result, anyhow};
use colored::*;
use serde::Deserialize;
use serde_yaml::{Value, from_value};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::config::{ConfigData, ConfigSpec, TestStepGroupReference};
use crate::test_step::{
    AssertionResult, RunnableTestStep, TestStep, TestStepFailureReason, TestStepResult,
    TestStepSpec,
};

#[derive(Clone)]
pub struct Test {
    pub name: String,
    path: PathBuf,
    pub config: Option<Arc<RwLock<ConfigData>>>,
    pub groups: Option<Vec<String>>,
    setup: Option<String>,
    teardown: Option<String>,
    steps: Vec<Arc<RwLock<dyn RunnableTestStep + Send + Sync>>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestSpec {
    setup: Option<String>,
    teardown: Option<String>,
    steps: Vec<Value>,
    config: Option<ConfigSpec>,
    groups: Option<Vec<String>>,
}

pub fn print_test_results(test_results: &[TestResult], duration_secs: f32, verbosity: u8) {
    if verbosity == 0 {
        return;
    }

    let mut num_passes = 0usize;
    let mut fails: Vec<&TestResult> = vec![];
    for r in test_results {
        if r.passed() { num_passes += 1; } else { fails.push(r); }
    }

    let total = test_results.len();
    let num_failures = fails.len();
    let divider = "─".repeat(40);

    println!();
    println!("{}", divider.dimmed());

    if num_failures == 0 {
        println!(
            "{}  ({} total, {:.2}s)",
            format!("Results: {} passed", num_passes).green(),
            total,
            duration_secs
        );
    } else {
        println!(
            "Results: {}  ({} total, {:.2}s)",
            format!("{} passed, {} failed", num_passes, num_failures).red(),
            total,
            duration_secs
        );
    }

    println!("{}", divider.dimmed());

    // Verbosity 2+ shows the FAILURES detail block.
    if verbosity >= 2 && !fails.is_empty() {
        println!();
        println!("{}", "FAILURES".bold());
        for failure in &fails {
            println!();
            println!("  {} {}", "✗".red(), failure.test_name.bold());
            println!("    File:  {}", failure.test_path.display());
            if let Some(msg) = failure.get_failure_message() {
                println!("    Error: {}", msg);
            }
        }
        println!();
    }
}

pub struct TestResult {
    test_name: String,
    test_path: PathBuf,
    /// All steps that ran (including the failing one if any).
    pub steps: Vec<TestStepResult>,
    success: bool,
    pub duration_ms: u64,
}

impl TestResult {
    pub fn name(&self) -> &str {
        &self.test_name
    }

    pub fn get_failure_message(&self) -> Option<&str> {
        if self.success {
            return None;
        }
        self.steps.last().and_then(|s| s.failure_message.as_deref())
    }

    pub fn assertions(&self) -> impl Iterator<Item = &AssertionResult> {
        self.steps.iter().flat_map(|s| s.assertion_results.iter())
    }

    pub fn passed(&self) -> bool {
        self.success
    }

    pub fn file_path(&self) -> Option<&PathBuf> {
        Some(&self.test_path)
    }

    fn make_failure(
        test_name: &String,
        test_path: &PathBuf,
        steps: Vec<TestStepResult>,
    ) -> TestResult {
        TestResult {
            test_name: test_name.to_string(),
            test_path: test_path.to_path_buf(),
            steps,
            success: false,
            duration_ms: 0,
        }
    }
}

fn is_test_name(key: &str) -> bool {
    let lower_name = key.to_lowercase();
    lower_name.starts_with("test") || lower_name.ends_with("test")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_step::{AssertionResult, TestStepFailureReason, TestStepResult};

    fn make_step_failure(msg: &str) -> TestStepResult {
        TestStepResult::make_failure(
            Some("step"),
            TestStepFailureReason::StatusCodeError,
            msg.to_owned(),
        )
    }

    // ── is_test_name ─────────────────────────────────────────────────────────

    #[test]
    fn test_is_test_name_prefix() {
        assert!(is_test_name("test_login"));
        assert!(is_test_name("Test Login"));
        assert!(is_test_name("TEST_CREATE_USER"));
    }

    #[test]
    fn test_is_test_name_suffix() {
        assert!(is_test_name("login_test"));
        assert!(is_test_name("create user test"));
    }

    #[test]
    fn test_is_test_name_neither() {
        assert!(!is_test_name("config"));
        assert!(!is_test_name("setup"));
        assert!(!is_test_name("login_flow"));
    }

    #[test]
    fn test_is_test_name_case_insensitive() {
        assert!(is_test_name("TEST"));
        assert!(is_test_name("MYTEST"));
    }

    // ── TestResult ───────────────────────────────────────────────────────────

    #[test]
    fn test_result_passed_is_true_on_success() {
        let result = TestResult {
            test_name: "my test".to_owned(),
            test_path: PathBuf::from("/test.yaml"),
            steps: vec![],
            success: true,
            duration_ms: 0,
        };
        assert!(result.passed());
    }

    #[test]
    fn test_result_get_failure_message_returns_none_on_success() {
        let result = TestResult {
            test_name: "my test".to_owned(),
            test_path: PathBuf::from("/test.yaml"),
            steps: vec![],
            success: true,
            duration_ms: 0,
        };
        assert!(result.get_failure_message().is_none());
    }

    #[test]
    fn test_result_get_failure_message_returns_last_step_message() {
        let step = make_step_failure("expected 200, got 404");
        let result = TestResult {
            test_name: "my test".to_owned(),
            test_path: PathBuf::from("/test.yaml"),
            steps: vec![step],
            success: false,
            duration_ms: 0,
        };
        assert_eq!(result.get_failure_message(), Some("expected 200, got 404"));
    }

    #[test]
    fn test_result_assertions_iterates_all_steps() {
        let mut step1 = make_step_failure("bad status");
        step1.assertion_results = vec![
            AssertionResult { name: "status 200".to_owned(), passed: false, message: Some("bad status".to_owned()) },
        ];
        let mut step2 = make_step_failure("bad body");
        step2.assertion_results = vec![
            AssertionResult { name: "name".to_owned(), passed: true, message: None },
            AssertionResult { name: "age".to_owned(), passed: false, message: Some("bad body".to_owned()) },
        ];
        let result = TestResult {
            test_name: "t".to_owned(),
            test_path: PathBuf::from("/t.yaml"),
            steps: vec![step1, step2],
            success: false,
            duration_ms: 0,
        };
        assert_eq!(result.assertions().count(), 3);
    }
}

impl Test {
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn add_config(&mut self, config: Arc<RwLock<ConfigData>>) {
        match &self.config {
            Some(cfg) => {
                enum Relation {
                    NewIsParent,
                    CurrentIsParent,
                    Unrelated(String, String),
                    NoParents,
                }

                let relation = {
                    let new_guard = config.read().unwrap();
                    let cfg_guard = cfg.read().unwrap();
                    match (new_guard.path.parent(), cfg_guard.path.parent()) {
                        (Some(new_dir), Some(current_dir)) => {
                            if current_dir.starts_with(new_dir) {
                                Relation::NewIsParent
                            } else if new_dir.starts_with(current_dir) {
                                Relation::CurrentIsParent
                            } else {
                                Relation::Unrelated(
                                    new_dir.display().to_string(),
                                    current_dir.display().to_string(),
                                )
                            }
                        }
                        _ => Relation::NoParents,
                    }
                };

                match relation {
                    Relation::NewIsParent => cfg.write().unwrap().set_parent(config),
                    Relation::CurrentIsParent => {
                        config.write().unwrap().set_parent(Arc::clone(cfg));
                    }
                    Relation::Unrelated(a, b) => panic!(
                        "ERROR: Cannot set parentage with unrelated configs {} {}",
                        a, b
                    ),
                    Relation::NoParents => {}
                }
            }
            None => {
                self.config = Some(Arc::clone(&config));
            }
        }
    }

    pub fn from_spec(path: PathBuf, name: String, spec: TestSpec) -> Result<Test> {
        let mut config: Option<Arc<RwLock<ConfigData>>> = None;
        if let Some(config_spec) = spec.config {
            let loaded_config = ConfigData::from_spec(&path, config_spec)?;
            config = Some(Arc::new(RwLock::new(loaded_config)));
        }

        let mut test_steps: Vec<Arc<RwLock<dyn RunnableTestStep + Send + Sync>>> = vec![];

        for step in spec.steps {
            // Check for a plain string reference first so we can move the value
            // into from_value without cloning when it's a structured step spec.
            if let Some(step_name) = step.as_str() {
                test_steps.push(Arc::new(RwLock::new(
                    TestStepGroupReference::from_id(step_name.to_owned()),
                )));
            } else {
                match from_value::<TestStepSpec>(step) {
                    Ok(test_step_spec) => {
                        test_steps.push(Arc::new(RwLock::new(TestStep::from_spec(test_step_spec))));
                    }
                    Err(_) => return Err(anyhow!("Error Decoding Step in test {}", name)),
                }
            }
        }

        Ok(Test {
            name,
            path,
            setup: spec.setup,
            teardown: spec.teardown,
            steps: test_steps,
            config,
            groups: spec.groups,
        })
    }

    pub fn load_from_value(
        mut value: Value,
        path: &PathBuf,
    ) -> Result<(Option<ConfigData>, Vec<Test>), Error> {
        let mut config: Option<ConfigData> = None;
        let mut tests: Vec<Test> = vec![];

        if let Value::Mapping(ref mut mapping) = value {
            if let Some(config_value) = mapping.remove("config") {
                config = Some(ConfigData::from_val(config_value, path)?);
            }
        }

        if let Value::Mapping(mapping) = value {
            for (key_val, val) in mapping {
                if let Some(key) = key_val.as_str() {
                    if is_test_name(key) {
                        match from_value::<TestSpec>(val) {
                            Ok(test_spec) => {
                                let test =
                                    Test::from_spec(path.clone(), key.to_owned(), test_spec)?;
                                tests.push(test);
                            }
                            Err(e) => {
                                return Err(anyhow!("Failed to parse test '{}': {}", key, e));
                            }
                        }
                    }
                }
            }
        }

        Ok((config, tests))
    }

    pub fn load_from_file(path: &PathBuf) -> Result<(Option<ConfigData>, Vec<Test>), Error> {
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            match serde_yaml::from_reader::<_, Value>(reader) {
                Ok(value) => Test::load_from_value(value, path),
                Err(e) => Err(Error::from(e)),
            }
        } else {
            Ok((None, vec![]))
        }
    }

    pub async fn run(&self) -> TestResult {
        let mut prior_steps: HashMap<String, TestStepResult> = HashMap::new();
        let mut completed_steps: Vec<TestStepResult> = Vec::new();

        macro_rules! fail {
            ($step_result:expr) => {{
                completed_steps.push($step_result);
                return TestResult::make_failure(&self.name, &self.path, completed_steps);
            }};
        }

        // Setup
        if let (Some(setup_id), Some(cfg)) = (self.setup.as_deref(), &self.config) {
            match cfg.read().unwrap().get_step_group(setup_id) {
                Ok(setup) => match setup.run(&self.config, &prior_steps).await {
                    Ok(result) => {
                        prior_steps.insert("setup".to_owned(), result.clone());
                        completed_steps.push(result);
                    }
                    Err(e) => fail!(TestStepResult::make_failure(
                        Some("setup"),
                        TestStepFailureReason::Miscellaneous,
                        format!("setup failed: {}", e),
                    )),
                },
                Err(e) => fail!(TestStepResult::make_failure(
                    Some("setup"),
                    TestStepFailureReason::SharedStepNotFoundError,
                    format!("setup step-set not found: {}", e),
                )),
            }
        }

        // Steps
        for step in self.steps.iter() {
            let real_step = step.read().unwrap();
            match real_step.run(&self.config, &prior_steps).await {
                Ok(result) => {
                    if result.status != TestStepFailureReason::NoFailure {
                        fail!(result);
                    }
                    if let Some(id) = real_step.get_id() {
                        prior_steps.insert(id.clone(), result.clone());
                    }
                    completed_steps.push(result);
                }
                Err(e) => {
                    let step_id = real_step.get_id().map(String::as_str);
                    fail!(TestStepResult::make_failure(
                        step_id,
                        TestStepFailureReason::Miscellaneous,
                        format!("step failed: {}", e),
                    ));
                }
            }
        }

        // Teardown
        if let (Some(teardown_id), Some(cfg)) = (self.teardown.as_deref(), &self.config) {
            match cfg.read().unwrap().get_step_group(teardown_id) {
                Ok(teardown) => match teardown.run(&self.config, &prior_steps).await {
                    Ok(result) => {
                        prior_steps.insert("teardown".to_owned(), result.clone());
                        completed_steps.push(result);
                    }
                    Err(e) => fail!(TestStepResult::make_failure(
                        Some("teardown"),
                        TestStepFailureReason::Miscellaneous,
                        format!("teardown failed: {}", e),
                    )),
                },
                Err(e) => fail!(TestStepResult::make_failure(
                    Some("teardown"),
                    TestStepFailureReason::SharedStepNotFoundError,
                    format!("teardown step-set not found: {}", e),
                )),
            }
        }

        TestResult {
            test_name: self.name.clone(),
            test_path: self.path.clone(),
            steps: completed_steps,
            success: true,
            duration_ms: 0,
        }
    }
}
