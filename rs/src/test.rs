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
    RunnableTestStep, TestStep, TestStepFailureReason, TestStepResult, TestStepSpec,
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

pub fn print_test_results(test_results: &[TestResult], duration_secs: f32) {
    let mut passes: Vec<&TestResult> = vec![];
    let mut fails: Vec<&TestResult> = vec![];

    for test_result in test_results {
        if test_result.passed() {
            passes.push(test_result);
        } else {
            fails.push(test_result);
        }
    }

    let total = test_results.len();
    let num_passes = passes.len();
    let num_failures = fails.len();
    let divider = "─".repeat(40);

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

    if !fails.is_empty() {
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
    failing_step: Option<TestStepResult>,
    success: bool,
}

impl TestResult {
    pub fn name(&self) -> &str {
        &self.test_name
    }

    pub fn get_failure_message(&self) -> Option<&str> {
        self.failing_step
            .as_ref()
            .and_then(|s| s.failure_message.as_deref())
    }

    pub fn passed(&self) -> bool {
        self.success
    }

    pub fn make_failure_from_error(
        test_name: &String,
        test_path: &PathBuf,
        step_id: Option<String>,
        failure_reason: TestStepFailureReason,
        error_prefix: String,
        error: Error,
    ) -> TestResult {
        let step_result = TestStepResult::make_failure(
            &step_id,
            failure_reason,
            format!("{}: {}", error_prefix, error),
        );
        TestResult {
            test_name: test_name.to_string(),
            test_path: test_path.to_path_buf(),
            failing_step: Some(step_result),
            success: false,
        }
    }

    pub fn make_failure(
        test_name: &String,
        test_path: &PathBuf,
        failure: TestStepResult,
    ) -> TestResult {
        TestResult {
            test_name: test_name.to_string(),
            test_path: test_path.to_path_buf(),
            failing_step: Some(failure),
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
                let o_new_config_dir = config.read().unwrap().path.clone();
                let o_current_config_dir = cfg.read().unwrap().path.clone();

                if let (Some(new_dir), Some(current_dir)) =
                    (o_new_config_dir.parent(), o_current_config_dir.parent())
                {
                    if current_dir.starts_with(new_dir) {
                        cfg.write().unwrap().set_parent(config);
                    } else if new_dir.starts_with(current_dir) {
                        config.write().unwrap().set_parent(Arc::clone(cfg));
                    } else {
                        panic!(
                            "ERROR: Cannot set parentage with unrelated configs {} {}",
                            new_dir.display(),
                            current_dir.display()
                        );
                    }
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
                Ok(test_file) => {
                    if let Some(config_value) = test_file.get("config") {
                        config = Some(ConfigData::from_val(config_value, path)?);
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

        // Use as_deref() to get Option<&str> from Option<String>, avoiding a clone.
        if let (Some(setup_id), Some(cfg)) = (self.setup.as_deref(), &self.config) {
            match cfg.read().unwrap().get_step_group(setup_id) {
                Ok(setup) => match setup.run(&self.config, &prior_steps).await {
                    Ok(result) => {
                        prior_steps.insert("setup".to_owned(), result);
                    }
                    Err(e) => {
                        return TestResult::make_failure_from_error(
                            &self.name,
                            &self.path,
                            Some("setup".to_owned()),
                            TestStepFailureReason::Miscellaneous,
                            "setup failed".to_owned(),
                            e,
                        );
                    }
                },
                Err(e) => {
                    return TestResult::make_failure_from_error(
                        &self.name,
                        &self.path,
                        Some("setup".to_owned()),
                        TestStepFailureReason::SharedStepNotFoundError,
                        "setup step-set not found".to_owned(),
                        e,
                    );
                }
            }
        }

        for step in self.steps.iter() {
            let real_step = step.read().unwrap();
            match real_step.run(&self.config, &prior_steps).await {
                Ok(result) => {
                    if result.status != TestStepFailureReason::NoFailure {
                        return TestResult::make_failure(&self.name, &self.path, result);
                    } else if let Some(id) = real_step.get_id() {
                        prior_steps.insert(id.clone(), result);
                    }
                }
                Err(e) => {
                    let step_id = real_step.get_id().cloned();
                    return TestResult::make_failure_from_error(
                        &self.name,
                        &self.path,
                        step_id,
                        TestStepFailureReason::Miscellaneous,
                        "step failed".to_owned(),
                        e,
                    );
                }
            }
        }

        if let (Some(teardown_id), Some(cfg)) = (self.teardown.as_deref(), &self.config) {
            match cfg.read().unwrap().get_step_group(teardown_id) {
                Ok(teardown) => match teardown.run(&self.config, &prior_steps).await {
                    Ok(result) => {
                        prior_steps.insert("teardown".to_owned(), result);
                    }
                    Err(e) => {
                        return TestResult::make_failure_from_error(
                            &self.name,
                            &self.path,
                            Some("teardown".to_owned()),
                            TestStepFailureReason::Miscellaneous,
                            "teardown failed".to_owned(),
                            e,
                        );
                    }
                },
                Err(e) => {
                    return TestResult::make_failure_from_error(
                        &self.name,
                        &self.path,
                        Some("teardown".to_owned()),
                        TestStepFailureReason::SharedStepNotFoundError,
                        "teardown step-set not found".to_owned(),
                        e,
                    );
                }
            }
        }

        TestResult {
            test_name: self.name.clone(),
            test_path: self.path.clone(),
            failing_step: None,
            success: true,
        }
    }
}
