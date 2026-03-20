use crate::test_step::{RunnableTestStep, TestStep, TestStepSpec, TestStepStatus};
use serde::Deserialize;
use serde_yaml::{Value, from_value};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestStepGroupSpec {
    steps: Vec<Value>,
    output: HashMap<String, String>,
    once: Option<bool>,
}

pub struct TestStepGroup {
    id: Option<String>,
    steps: Vec<Rc<dyn RunnableTestStep>>,
    status: TestStepStatus,
    run_once: bool,
    has_run: bool,
}

impl TestStepGroup {
    pub fn from_spec(id: String, spec: TestStepGroupSpec) -> TestStepGroup {
        let mut once = false;
        if let Some(run_once) = spec.once
            && run_once
        {
            once = true;
        }

        let mut steps: Vec<Rc<dyn RunnableTestStep>> = vec![];
        for step in spec.steps.iter() {
            if let Some(step_name) = step.as_str() {
                panic!("Need to implement step names {}", step_name);
            } else if let Ok(test_step_spec) = from_value::<TestStepSpec>(step.clone()) {
                let test_step = TestStep::from_spec(test_step_spec);
                let test_step_rc: Rc<TestStep> = Rc::new(test_step);
                steps.push(test_step_rc);
            }
        }

        TestStepGroup {
            id: Some(id),
            steps,
            status: TestStepStatus::NotRun,
            run_once: once,
            has_run: false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigSpec {
    step_sets: Option<HashMap<String, TestStepGroupSpec>>,
    vars: Option<HashMap<String, String>>,
    urls: Option<HashMap<String, String>>,
}

pub struct ConfigData {
    path: PathBuf,
    parent: Option<Rc<ConfigData>>,
    step_sets: Option<HashMap<String, TestStepGroup>>,
    vars: Option<HashMap<String, String>>,
    urls: Option<HashMap<String, String>>,
}

impl ConfigData {
    pub fn from_config_spec(
        path: &PathBuf,
        parent: Option<Rc<ConfigData>>,
        spec: ConfigSpec,
    ) -> ConfigData {
        let mut step_sets: Option<HashMap<String, TestStepGroup>> = None;
        if let Some(step_set_specs) = spec.step_sets {
            step_sets = Some(
                step_set_specs
                    .into_iter()
                    .map(|(k, v)| (k.clone(), TestStepGroup::from_spec(k.clone(), v)))
                    .collect(),
            )
        }

        ConfigData {
            path: path.clone(),
            parent,
            step_sets,
            vars: spec.vars,
            urls: spec.urls,
        }
    }

    pub fn spec_from_value(value: Value) -> Option<ConfigSpec> {
        match from_value::<ConfigSpec>(value.clone()) {
            Ok(config_spec) => Some(config_spec),
            Err(e) => return None,
        }
    }

    pub fn spec_from_file(path: &PathBuf) -> Option<ConfigSpec> {
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            let config_file_result = serde_yaml::from_reader::<_, Value>(reader);
            match config_file_result {
                Ok(config_file) => {
                    return ConfigData::spec_from_value(config_file);
                }
                Err(e) => {
                    eprintln!("Error Loading Config File: {}\n{}", path.display(), e);
                }
            }
        }
        return None;
    }

    pub fn from_value(parent: Option<Rc<ConfigData>>, value: Value) -> Option<ConfigData {
        if let Some(spec) = ConfigData::spec_from_value(value) {
            return Some(ConfigData::from_config_spec(path, parent, spec));
        }
        None
    }

    pub fn from_file(parent: Option<Rc<ConfigData>>, path: &PathBuf) -> Option<ConfigData> {
        if let Some(spec) = ConfigData::spec_from_file(path) {
            return Some(ConfigData::from_config_spec(path, parent, spec));
        }
        None
    }

    /*
    pub fn get_step_set(&self, key: String) -> Option<&TestStepGroup> {
        let retrieved_value = self.step_sets.get(&key);
        match retrieved_value {
            Some(_) => {
                return retrieved_value;
            }
            None => match &self.parent {
                Some(parent) => parent.get_step_set(key),
                None => None,
            },
        }
    }

    pub fn get_keys(&self, keys: Vec<String>) -> Option<&Value> {
        let mut current_value = &self.data;
        for key in keys.iter() {
            let opt_val = current_value.get(key);
            match opt_val {
                Some(val) => {
                    current_value = val;
                }
                None => return None,
            }
        }

        Some(current_value)
    }
    */
}

impl RunnableTestStep for TestStepGroup {
    fn get_id(&self) -> Option<&String> {
        self.id.as_ref()
    }

    fn run(&mut self) {
        if self.has_run && self.run_once {
            return;
        }
        self.status = TestStepStatus::InProgress;
        for step in self.steps.iter_mut() {
            step.run();
            if step.get_status() == TestStepStatus::Fail {
                self.status = TestStepStatus::Fail;
                return;
            }
        }
        self.status = TestStepStatus::Pass;
    }

    fn get_status(&self) -> TestStepStatus {
        self.status
    }
}
