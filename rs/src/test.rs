use anyhow::{Error, Result, anyhow};
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

pub struct TestResult {
    failing_step: Option<TestStepResult>,
    success: bool,
}

impl TestResult {
    pub fn make_failure_from_error(
        step_id: Option<String>,
        failure_reason: TestStepFailureReason,
        error_prefix: String,
        error: Error,
    ) -> TestResult {
        let step_result = TestStepResult::make_failure(
            &step_id,
            failure_reason,
            format!("{}: ({})", error_prefix, error),
        );
        TestResult {
            failing_step: Some(step_result),
            success: false,
        }
    }

    pub fn make_failure(failure: TestStepResult) -> TestResult {
        TestResult {
            failing_step: Some(failure),
            success: false,
        }
    }
}

fn is_test_name(key: String) -> bool {
    let lower_name = key.to_lowercase();
    lower_name.starts_with("test") || lower_name.ends_with("test")
}

impl Test {
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

        for step in spec.steps.into_iter() {
            match from_value::<TestStepSpec>(step.clone()) {
                Ok(test_step_spec) => {
                    let step = TestStep::from_spec(test_step_spec);
                    test_steps.push(Arc::new(RwLock::new(step)));
                }
                Err(e) => {
                    // Possible that it's using a test step defined in the config
                    match step.clone().as_str() {
                        Some(step_name) => {
                            let step = TestStepGroupReference::from_id(step_name.to_string());
                            test_steps.push(Arc::new(RwLock::new(step)));
                        }
                        None => return Err(anyhow!("Error Decoding Step in test {}", name)),
                    }
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
                        config = Some(ConfigData::from_val(&config_value, path)?);
                    }

                    if let Some(mapping) = test_file.as_mapping() {
                        for key in mapping.keys().filter_map(|v| v.as_str()) {
                            if is_test_name(key.to_string()) {
                                if let Some(test_value) = mapping.get(key) {
                                    match from_value::<TestSpec>(test_value.clone()) {
                                        Ok(test_spec) => {
                                            let test = Test::from_spec(
                                                path.clone(),
                                                key.to_string(),
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

        if let (Some(setup_id), Some(cfg)) = (self.setup.clone(), &self.config) {
            match cfg.read().unwrap().get_step_group(setup_id.clone()) {
                Ok(setup) => match setup.run(&self.config, &prior_steps).await {
                    Ok(result) => {
                        prior_steps.insert("setup".to_string(), result);
                    }
                    Err(e) => {
                        return TestResult::make_failure_from_error(
                            Some("setup".to_string()),
                            TestStepFailureReason::Miscellaneous,
                            "Step Failed to Run".to_string(),
                            e,
                        );
                    }
                },
                Err(e) => {
                    return TestResult::make_failure_from_error(
                        Some("setup".to_string()),
                        TestStepFailureReason::SharedStepNotFoundError,
                        "Step Group Not Found".to_string(),
                        e,
                    );
                }
            }
        }

        for step in self.steps.iter() {
            let real_step = step.read().unwrap();
            // println!("Running Step");
            match real_step.run(&self.config, &prior_steps).await {
                Ok(result) => {
                    if result.status != TestStepFailureReason::NoFailure {
                        return TestResult::make_failure(result);
                    } else {
                        if let Some(id) = real_step.get_id() {
                            prior_steps.insert(id.clone(), result);
                        }
                    }
                }
                Err(e) => {
                    let mut step_id: Option<String> = None;
                    if let Some(actual_step_id) = real_step.get_id() {
                        step_id = Some(actual_step_id.clone());
                    }
                    return TestResult::make_failure_from_error(
                        step_id,
                        TestStepFailureReason::Miscellaneous,
                        "Step Failed to Run".to_string(),
                        e,
                    );
                }
            }
        }

        if let (Some(teardown_id), Some(cfg)) = (self.teardown.clone(), &self.config) {
            //println!("Running Setup");
            match cfg.read().unwrap().get_step_group(teardown_id.clone()) {
                Ok(teardown) => match teardown.run(&self.config, &prior_steps).await {
                    Ok(result) => {
                        prior_steps.insert("teardown".to_string(), result);
                    }
                    Err(e) => {
                        return TestResult::make_failure_from_error(
                            Some("teardown".to_string()),
                            TestStepFailureReason::Miscellaneous,
                            "Test Failed to Run".to_string(),
                            e,
                        );
                    }
                },
                Err(e) => {
                    return TestResult::make_failure_from_error(
                        Some("teardown".to_string()),
                        TestStepFailureReason::SharedStepNotFoundError,
                        "Step Group Not Found".to_string(),
                        e,
                    );
                }
            }
        }
        return TestResult {
            failing_step: None,
            success: true,
        };
    }
}
