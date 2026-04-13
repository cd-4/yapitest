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
        }
    }
}

fn is_test_name(key: &str) -> bool {
    let lower_name = key.to_lowercase();
    lower_name.starts_with("test") || lower_name.ends_with("test")
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

    pub fn load_from_file(path: &PathBuf) -> Result<(Option<ConfigData>, Vec<Test>), Error> {
        let mut config: Option<ConfigData> = None;
        let mut tests: Vec<Test> = vec![];

        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            let test_file_result = serde_yaml::from_reader::<_, Value>(reader);
            match test_file_result {
                Ok(mut test_file) => {
                    // Extract the config entry by value (removing it from the mapping)
                    // so we can pass it to from_val without cloning.
                    if let Value::Mapping(ref mut mapping) = test_file {
                        if let Some(config_value) = mapping.remove("config") {
                            config = Some(ConfigData::from_val(config_value, path)?);
                        }
                    }

                    // Consume the mapping so each test's Value can be moved
                    // into from_value without cloning.
                    if let Value::Mapping(mapping) = test_file {
                        for (key_val, value) in mapping {
                            if let Some(key) = key_val.as_str() {
                                if is_test_name(key) {
                                    match from_value::<TestSpec>(value) {
                                        Ok(test_spec) => {
                                            let test = Test::from_spec(
                                                path.clone(),
                                                key.to_owned(),
                                                test_spec,
                                            )?;
                                            tests.push(test);
                                        }
                                        Err(e) => {
                                            panic!(
                                                "Failed to parse test: {} at {}\n{}",
                                                key,
                                                path.display(),
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(Error::from(e));
                }
            }
        }
        Ok((config, tests))
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
        }
    }
}
