use anyhow::{Error, Result};
use serde::Deserialize;
use serde_yaml::{Value, from_value};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use crate::config::{ConfigData, ConfigSpec};
use crate::test_step::{TestStep, TestStepSpec};

pub struct Test {
    name: String,
    path: PathBuf,
    config: Option<ConfigData>,
    groups: Option<Vec<String>>,
    setup: Option<String>,
    teardown: Option<String>,
    steps: Vec<TestStep>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestSpec {
    setup: Option<String>,
    teardown: Option<String>,
    steps: Vec<TestStepSpec>,
    config: Option<ConfigSpec>,
    groups: Option<Vec<String>>,
}

fn is_test_name(key: String) -> bool {
    let lower_name = key.to_lowercase();
    lower_name.starts_with("test") || lower_name.ends_with("test")
}

impl Test {
    pub fn has_config(&self) -> bool {
        self.config.is_some()
    }

    pub fn set_config(&mut self, config: &ConfigData) {
        self.config = Some(config);
    }

    pub fn from_spec(path: PathBuf, name: String, spec: TestSpec) -> Test {
        let mut config: Option<ConfigData> = None;
        if let Some(config_spec) = spec.config {
            config = Some(ConfigData::from_config_spec(&path, None, config_spec));
        }
        Test {
            name,
            path,
            setup: spec.setup,
            teardown: spec.teardown,
            steps: spec.steps.into_iter().map(TestStep::from_spec).collect(),
            config,
            groups: spec.groups,
        }
    }

    pub fn load_from_file(path: &PathBuf) -> Result<(Option<ConfigData>, Vec<Test>), Error> {
        let mut config: Option<ConfigData> = None;
        let mut tests: Vec<Test> = vec![];

        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            let test_file_result = serde_yaml::from_reader::<_, Value>(reader);
            match test_file_result {
                Ok(test_file) => {
                    println!("Loaded Raw Test File");
                    if let Some(config_value) = test_file.get("config") {
                        config = Some(ConfigData::from_val(&config_value, path)?);
                    }

                    if let Some(mapping) = test_file.as_mapping() {
                        for key in mapping.keys().filter_map(|v| v.as_str()) {
                            if is_test_name(key.to_string()) {
                                if let Some(test_value) = mapping.get(key) {
                                    match from_value::<TestSpec>(test_value.clone()) {
                                        Ok(test_spec) => {
                                            tests.push(Test::from_spec(
                                                path.clone(),
                                                key.to_string(),
                                                test_spec,
                                            ));
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

    pub fn load_test_file(path: &PathBuf) -> (Option<ConfigData>, Vec<Test>) {
        let mut config: Option<ConfigData> = None;
        let mut tests: Vec<Test> = vec![];

        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            let test_file_result = serde_yaml::from_reader::<_, Value>(reader);
            match test_file_result {
                Ok(tests_file) => {
                    println!("Loaded Test File");

                    if let Some(config_value) = tests_file.get("config") {
                        config = ConfigData::from_value(None, config_value.clone(), path);
                    }

                    if let Some(mapping) = tests_file.as_mapping() {
                        for key in mapping.keys().filter_map(|v| v.as_str()) {
                            if is_test_name(key.to_string()) {
                                if let Some(test_value) = mapping.get(key) {
                                    match from_value::<TestSpec>(test_value.clone()) {
                                        Ok(test_spec) => {
                                            tests.push(Test::from_spec(
                                                path.clone(),
                                                key.to_string(),
                                                test_spec,
                                            ));
                                            //println!("Loaded Test Data: {:?}", test_spec);
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
                            eprintln!("Key: {}", key);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error Loading Test File: {}\n{}", path.display(), e);
                }
            }
        }
        (config, tests)
    }
}
