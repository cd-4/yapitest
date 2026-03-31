use crate::test_step::{RunnableTestStep, TestStep, TestStepSpec, TestStepStatus};
use anyhow::{Error, Result, anyhow};
use serde::Deserialize;
use serde_yaml::{Value, from_value};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

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

pub struct ConfigVariable {
    value: Option<String>,
    env_var_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigSpec {
    step_sets: Option<HashMap<String, TestStepGroupSpec>>,
    vars: Option<HashMap<String, Value>>,
    urls: Option<HashMap<String, Value>>,
}

#[derive(Copy)]
pub struct ConfigData {
    pub path: PathBuf,
    parent: Option<&ConfigData>,
    step_sets: Option<HashMap<String, TestStepGroup>>,
    vars: HashMap<String, String>,
    urls: HashMap<String, String>,
}

impl ConfigData {
    pub fn set_parent(&mut self, parent: &ConfigData) {
        self.parent = Some(parent);
    }

    fn create_variables(
        spec_vars: HashMap<String, Value>,
    ) -> Result<HashMap<String, String>, Error> {
        let mut output: HashMap<String, String> = HashMap::new();

        for (key, value) in spec_vars.iter() {
            if let Some(string_val) = value.as_str() {
                output.insert(String::from(key), String::from(string_val));
            } else if let Some(mapping_val) = value.as_mapping() {
                let mut has_value = false;
                if let Some(env_var_name_str) = mapping_val.get("env").and_then(|v| v.as_str()) {
                    if let Ok(env_var_str) = std::env::var(env_var_name_str) {
                        output.insert(String::from(key), env_var_str);
                        has_value = true;
                    }
                }

                if !has_value
                    && let Some(default_str) = mapping_val.get("default").and_then(|v| v.as_str())
                {
                    output.insert(String::from(key), String::from(default_str));
                    has_value = true;
                }

                if !has_value {
                    let error_message = format!(
                        "\
                        Variable ({}) must be set to either a string value, \
                        or a mapping with one or more of 'default' and 'env' values.",
                        key
                    );
                    return Err(anyhow!(error_message));
                }
            }
        }

        Ok(output)
    }

    pub fn from_spec(path: &PathBuf, spec: ConfigSpec) -> Result<ConfigData> {
        let mut step_sets: Option<HashMap<String, TestStepGroup>> = None;
        if let Some(step_set_specs) = spec.step_sets {
            step_sets = Some(
                step_set_specs
                    .into_iter()
                    .map(|(k, v)| (k.clone(), TestStepGroup::from_spec(k.clone(), v)))
                    .collect(),
            )
        }

        let mut vars: HashMap<String, String> = HashMap::new();
        let mut urls: HashMap<String, String> = HashMap::new();

        if let Some(spec_vars) = spec.vars {
            match ConfigData::create_variables(spec_vars) {
                Ok(vars_result) => {
                    vars = vars_result;
                }
                Err(e) => {
                    return Err(anyhow!("Error Decoding Config {}:\n{}", path.display(), e));
                }
            }
        }

        if let Some(spec_urls) = spec.urls {
            match ConfigData::create_variables(spec_urls) {
                Ok(urls_result) => {
                    urls = urls_result;
                }
                Err(e) => {
                    return Err(anyhow!("Error Decoding Config {}:\n{}", path.display(), e));
                }
            }
        }

        Ok(ConfigData {
            path: path.clone(),
            parent: None,
            step_sets,
            vars,
            urls,
        })
    }

    pub fn from_config_spec(
        path: &PathBuf,
        parent: Option<Arc<Mutex<ConfigData>>>,
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

        let mut vars: HashMap<String, String> = HashMap::new();
        let mut urls: HashMap<String, String> = HashMap::new();

        if let Some(spec_vars) = spec.vars {
            match ConfigData::create_variables(spec_vars) {
                Ok(vars_result) => {
                    vars = vars_result;
                }
                Err(e) => {
                    panic!("Error Decoding Config {}:\n{}", path.display(), e);
                }
            }
        }

        if let Some(spec_urls) = spec.urls {
            match ConfigData::create_variables(spec_urls) {
                Ok(urls_result) => {
                    urls = urls_result;
                }
                Err(e) => {
                    panic!("Error Decoding Config {}:\n{}", path.display(), e);
                }
            }
        }

        ConfigData {
            path: path.clone(),
            parent,
            step_sets,
            vars,
            urls,
        }
    }

    pub fn spec_from_val(value: &Value) -> anyhow::Result<ConfigSpec> {
        match from_value::<ConfigSpec>(value.clone()) {
            Ok(config_spec) => Ok(config_spec),
            Err(e) => Err(anyhow!("{}", e)),
        }
    }

    pub fn spec_from_value(value: Value) -> Option<ConfigSpec> {
        match from_value::<ConfigSpec>(value.clone()) {
            Ok(config_spec) => Some(config_spec),
            Err(e) => {
                eprintln!("Error Loading Config: {}", e);
                None
            }
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
        } else {
            eprintln!("Error Reading Config File: {}", path.display());
        }
        None
    }

    pub fn from_val(value: &Value, path: &PathBuf) -> Result<ConfigData> {
        ConfigData::spec_from_val(value).and_then(|v| ConfigData::from_spec(path, v))
    }

    pub fn from_value(
        parent: Option<Arc<Mutex<ConfigData>>>,
        value: Value,
        path: &PathBuf,
    ) -> Option<ConfigData> {
        if let Some(spec) = ConfigData::spec_from_value(value) {
            return Some(ConfigData::from_config_spec(path, parent, spec));
        }
        None
    }

    pub fn from_file_new(path: &PathBuf) -> Result<ConfigData> {}

    pub fn from_file(parent: Option<Arc<Mutex<ConfigData>>>, path: &PathBuf) -> Option<ConfigData> {
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
        /*
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
        */
    }

    fn get_status(&self) -> TestStepStatus {
        self.status
    }
}
