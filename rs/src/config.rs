use crate::test_step::{
    RunnableTestStep, TestStep, TestStepFailureReason, TestStepResult, TestStepSpec,
};
use anyhow::{Error, Result, anyhow};
use async_trait::async_trait;
use dashmap::DashMap;
use serde::Deserialize;
use serde_yaml::{Value, from_value};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, LazyLock, RwLock};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestStepGroupSpec {
    steps: Vec<Value>,
    output: HashMap<String, String>,
    once: Option<bool>,
}

#[derive(Clone)]
pub struct TestStepGroup {
    id: String,
    steps: Vec<Arc<dyn RunnableTestStep + Send + Sync>>,
    outputs: HashMap<String, String>,
    run_once: bool,
    path: PathBuf,
}

pub struct TestStepGroupReference {
    id: String,
}

static GROUP_TEST_RESULTS: LazyLock<DashMap<String, TestStepResult>> = LazyLock::new(DashMap::new);

impl TestStepGroup {
    pub fn from_spec(id: String, spec: TestStepGroupSpec, path: &PathBuf) -> TestStepGroup {
        let mut once = false;
        if let Some(run_once) = spec.once
            && run_once
        {
            once = true;
        }

        let mut steps: Vec<Arc<dyn RunnableTestStep + Send + Sync>> = vec![];
        for step in spec.steps.iter() {
            if let Some(step_name) = step.as_str() {
                panic!("Need to implement step names {}", step_name);
            } else if let Ok(test_step_spec) = from_value::<TestStepSpec>(step.clone()) {
                let test_step = TestStep::from_spec(test_step_spec);
                let test_step_rc: Arc<TestStep> = Arc::new(test_step);
                steps.push(test_step_rc);
            }
        }

        TestStepGroup {
            id,
            steps,
            run_once: once,
            outputs: spec.output,
            path: path.clone(),
        }
    }

    pub fn runs_once(&self) -> bool {
        return self.run_once;
    }

    pub fn get_group_id(&self) -> String {
        return format!("{}:{}", self.path.display(), self.id);
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigSpec {
    step_sets: Option<HashMap<String, TestStepGroupSpec>>,
    vars: Option<HashMap<String, Value>>,
    urls: Option<HashMap<String, Value>>,
}

pub struct ConfigData {
    pub path: PathBuf,
    pub parent: Option<Arc<RwLock<ConfigData>>>,
    step_sets: Option<HashMap<String, TestStepGroup>>,
    vars: HashMap<String, String>,
    urls: HashMap<String, String>,
}

impl ConfigData {
    pub fn get_step_group(&self, step_group_key: String) -> Result<TestStepGroup> {
        if let Some(step_group) = self.step_sets.as_ref().and_then(|v| v.get(&step_group_key)) {
            return Ok(step_group.clone());
        }

        if let Some(parent) = &self.parent {
            let r = parent.read();
            let u = r.unwrap();
            let step_group = u.get_step_group(step_group_key)?;
            return Ok(step_group.clone());
        }
        Err(anyhow!("Step Group {} Not Found", step_group_key))
    }

    pub fn get_string_value(&self, key: String) -> Result<String> {
        let string_keys: Vec<String> = key.split('.').map(|v| v.to_string()).collect();
        if string_keys[0] == "urls" {
            if let Some(val) = self.urls.get(&string_keys[1]) {
                if val.starts_with('$') {
                    let mut new_val = val.clone();
                    new_val.remove(0);
                    return self.get_string_value(new_val);
                }
                return Ok(val.clone());
            }
        }
        if string_keys[0] == "vars" {
            if let Some(var) = self.vars.get(&string_keys[1]) {
                if var.starts_with('$') {
                    let mut new_val = var.clone();
                    new_val.remove(0);
                    return self.get_string_value(new_val);
                }
                return Ok(var.clone());
            }
        }
        if let Some(par) = &self.parent {
            return par.read().unwrap().get_string_value(key);
        }
        Err(anyhow!("Url {} not found in any config", key))
    }

    pub fn set_parent(&mut self, parent: Arc<RwLock<ConfigData>>) {
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
                    .map(|(k, v)| (k.clone(), TestStepGroup::from_spec(k.clone(), v, path)))
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

    pub fn spec_from_val(value: &Value) -> anyhow::Result<ConfigSpec> {
        match from_value::<ConfigSpec>(value.clone()) {
            Ok(config_spec) => Ok(config_spec),
            Err(e) => Err(anyhow!("{}", e)),
        }
    }

    pub fn spec_from_file(path: &PathBuf) -> Result<ConfigSpec> {
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            let config_file_result = serde_yaml::from_reader::<_, Value>(reader);
            match config_file_result {
                Ok(config_file) => {
                    return ConfigData::spec_from_val(&config_file);
                }
                Err(e) => {
                    return Err(anyhow!(e));
                }
            }
        } else {
            return Err(anyhow!("Error Reading Config File: {}", path.display()));
        }
    }

    pub fn from_val(value: &Value, path: &PathBuf) -> Result<ConfigData> {
        ConfigData::spec_from_val(value).and_then(|v| ConfigData::from_spec(path, v))
    }

    pub fn from_file(path: &PathBuf) -> Result<ConfigData> {
        let spec = ConfigData::spec_from_file(path)?;
        ConfigData::from_spec(path, spec)
    }
}

impl TestStepGroupReference {
    pub fn from_id(id: String) -> TestStepGroupReference {
        TestStepGroupReference { id }
    }
}

#[async_trait]
impl RunnableTestStep for TestStepGroupReference {
    fn get_id(&self) -> Option<&String> {
        Some(&self.id)
    }

    async fn run(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> Result<TestStepResult> {
        Err(anyhow!("SDF"))
    }
}

#[async_trait]
impl RunnableTestStep for TestStepGroup {
    fn get_id(&self) -> Option<&String> {
        Some(&self.id)
    }

    async fn run(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> Result<TestStepResult> {
        let test_group_id = self.get_group_id();
        if self.runs_once() {
            if let Some(result) = GROUP_TEST_RESULTS.get(&test_group_id) {
                return Ok(result.clone());
            }
        }

        // Run Steps
        let mut local_steps: HashMap<String, TestStepResult> = HashMap::new();
        for step in self.steps.iter() {
            match step.run(config, prior_steps).await {
                Ok(result) => {
                    if let Some(id) = step.get_id() {
                        local_steps.insert(id.clone(), result);
                    }
                }
                Err(e) => {
                    return Err(anyhow!("Error running step: {}", e));
                }
            }
        }

        // Process Outputs
        let mut outputs: HashMap<String, serde_json::Value> = HashMap::new();

        for (output_key, output_value) in self.outputs.iter() {
            if output_value.starts_with('$') {
                let mut output_str_copy = output_value.clone();
                output_str_copy.remove(0);

                let mut output_sections: Vec<String> =
                    output_str_copy.split('.').map(|v| v.to_string()).collect();

                let mut step_id: String = "".to_string();

                if let Some(step_id_val) = output_sections.get(0) {
                    step_id = step_id_val.clone();
                } else {
                    return Err(anyhow!("Invalid Step Reference: {}", output_value));
                }

                if let Some(step) = local_steps.get(&step_id) {
                    output_sections.remove(0);
                    let field_key = output_sections.join(".");
                    if let Ok(val) = step.get_field(field_key.clone()) {
                        if let Some(yaml_val) = val {
                            if let Ok(v) = serde_json::from_value(yaml_val) {
                                outputs.insert(output_key.clone(), v);
                                continue;
                            }
                        }
                        return Err(anyhow!(
                            "Field {} not found in step {}",
                            output_key,
                            step_id,
                        ));
                    }
                } else {
                    return Err(anyhow!("Step {} not found.", step_id));
                }
            }
        }

        let result = TestStepResult::make_success(
            Some(self.id.clone()),
            serde_yaml::from_value(Value::Null)?,
            serde_yaml::from_value(Value::Null)?,
            serde_json::to_value(outputs)?,
        );

        if self.runs_once() {
            GROUP_TEST_RESULTS.insert(test_group_id, result.clone());
        }

        return Ok(result);
    }
}
