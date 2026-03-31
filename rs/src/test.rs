use anyhow::{Error, Result};
use serde::Deserialize;
use serde_yaml::{Value, from_value};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use crate::config::{ConfigData, ConfigSpec};
use crate::test_step::{TestStep, TestStepSpec};

pub struct Test {
    pub name: String,
    path: PathBuf,
    config: Option<Arc<RwLock<ConfigData>>>,
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

    pub fn has_config(&self) -> bool {
        self.config.is_some()
    }

    pub fn set_config(&mut self, config: Arc<RwLock<ConfigData>>) {
        match &self.config {
            Some(cfg) => {
                cfg.write().unwrap().set_parent(Arc::clone(&config));
            }
            None => {
                self.config = Some(Arc::clone(&config));
            }
        }
        self.config = Some(config);
    }

    pub fn from_spec(path: PathBuf, name: String, spec: TestSpec) -> Result<Test> {
        let mut config: Option<Arc<RwLock<ConfigData>>> = None;
        if let Some(config_spec) = spec.config {
            let loaded_config = ConfigData::from_spec(&path, config_spec)?;
            config = Some(Arc::new(RwLock::new(loaded_config)));
        }
        Ok(Test {
            name,
            path,
            setup: spec.setup,
            teardown: spec.teardown,
            steps: spec.steps.into_iter().map(TestStep::from_spec).collect(),
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
}
