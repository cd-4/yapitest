use crate::test_step::{resolve_value, RunnableTestStep, TestStep, TestStepResult, TestStepSpec};
use anyhow::{Error, Result, anyhow};
use async_trait::async_trait;
use dashmap::DashMap;
use lazy_static::lazy_static;
use serde::Deserialize;
use serde_yaml::{Value, from_value};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::OnceCell;

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

impl TestStepGroup {
    pub fn from_spec(id: &str, spec: TestStepGroupSpec, path: &PathBuf) -> TestStepGroup {
        let once = spec.once.unwrap_or(false);

        let mut steps: Vec<Arc<dyn RunnableTestStep + Send + Sync>> = vec![];
        for step in spec.steps {
            if let Some(step_name) = step.as_str() {
                panic!("Need to implement step names {}", step_name);
            } else if let Ok(test_step_spec) = from_value::<TestStepSpec>(step) {
                steps.push(Arc::new(TestStep::from_spec(test_step_spec)));
            }
        }

        TestStepGroup {
            id: id.to_owned(),
            steps,
            run_once: once,
            outputs: spec.output,
            path: path.clone(),
        }
    }

    pub fn runs_once(&self) -> bool {
        self.run_once
    }

    pub fn get_group_id(&self) -> String {
        format!("{}:{}", self.path.display(), self.id)
    }

    pub async fn run_internal(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
        args: Option<serde_json::Value>,
    ) -> Result<TestStepResult> {
        // Scope visible to the step-set's steps: the caller's prior steps plus
        // an optional `$args.*` namespace from a parameterized invocation.
        let mut scope: HashMap<String, TestStepResult> = prior_steps.clone();
        if let Some(args_val) = &args {
            scope.insert("args".to_owned(), TestStepResult::from_output(args_val.clone()));
        }

        // Run Steps
        let mut local_steps: HashMap<String, TestStepResult> = HashMap::new();
        for step in self.steps.iter() {
            match step.run(config, &scope).await {
                Ok(result) => {
                    if let Some(id) = step.get_id() {
                        local_steps.insert(id.to_string(), result);
                    }
                }
                Err(e) => {
                    let step_name = step.get_id().map(|s| s.as_str()).unwrap_or("(unnamed)");
                    return Err(anyhow!("step '{}' failed: {}", step_name, e));
                }
            }
        }

        // Output values resolve against the step-set's own steps plus `$args`.
        let mut output_scope = local_steps.clone();
        if let Some(args_val) = &args {
            output_scope.insert("args".to_owned(), TestStepResult::from_output(args_val.clone()));
        }

        // Process Outputs via the shared interpolation engine, so output values
        // support inline refs (`Bearer $login.response.token`) and literals.
        let mut outputs: HashMap<&str, serde_json::Value> = HashMap::new();

        for (output_key, output_value) in self.outputs.iter() {
            match resolve_value(output_value, config, &output_scope) {
                Ok(v) => {
                    outputs.insert(output_key.as_str(), v);
                }
                Err(e) => {
                    return Err(anyhow!("output '{}': {}", output_key, e));
                }
            }
        }

        let result = TestStepResult::make_success(
            Some(self.id.as_str()),
            serde_yaml::from_value(Value::Null)?,
            serde_yaml::from_value(Value::Null)?,
            serde_json::to_value(outputs)?,
        );
        Ok(result)
    }

    /// Run the step-set, optionally injecting an `$args.*` namespace. A
    /// parameterized invocation (args present) bypasses the once-cache, since
    /// different args produce different results.
    pub async fn run_with_args(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
        args: Option<serde_json::Value>,
    ) -> Result<TestStepResult> {
        if args.is_some() {
            return self.run_internal(config, prior_steps, args).await;
        }
        self.run(config, prior_steps).await
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
    pub fn get_step_group(&self, step_group_key: &str) -> Result<TestStepGroup> {
        if let Some(step_group) = self.step_sets.as_ref().and_then(|v| v.get(step_group_key)) {
            return Ok(step_group.clone());
        }

        if let Some(parent) = &self.parent {
            return parent.read().unwrap().get_step_group(step_group_key);
        }
        Err(anyhow!("step-set '{}' not found", step_group_key))
    }

    pub fn get_string_value(&self, key: &str) -> Result<String> {
        let mut parts = key.splitn(2, '.');
        let ns = parts.next().unwrap_or("");
        let field = parts.next().unwrap_or("");

        if ns == "urls" {
            if let Some(val) = self.urls.get(field) {
                return if val.starts_with('$') {
                    self.get_string_value(&val[1..])
                } else {
                    Ok(val.clone())
                };
            }
        }
        if ns == "vars" {
            if let Some(var) = self.vars.get(field) {
                return if var.starts_with('$') {
                    self.get_string_value(&var[1..])
                } else {
                    Ok(var.clone())
                };
            }
        }
        if let Some(par) = &self.parent {
            return par.read().unwrap().get_string_value(key);
        }
        Err(anyhow!("'{}' not found in any config", key))
    }

    pub fn set_parent(&mut self, parent: Arc<RwLock<ConfigData>>) {
        self.parent = Some(parent);
    }

    fn create_variables(
        spec_vars: HashMap<String, Value>,
    ) -> Result<HashMap<String, String>, Error> {
        let mut output: HashMap<String, String> = HashMap::new();

        for (key, value) in spec_vars {
            if let Some(string_val) = value.as_str() {
                output.insert(key, string_val.to_owned());
            } else if let Some(mapping_val) = value.as_mapping() {
                // Resolve env var first, fall back to default. Use or() so we
                // can move key into exactly one of insert or the error message.
                let env_val = mapping_val
                    .get("env")
                    .and_then(|v| v.as_str())
                    .and_then(|env_name| std::env::var(env_name).ok());

                let default_val = mapping_val
                    .get("default")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned());

                match env_val.or(default_val) {
                    Some(val) => {
                        output.insert(key, val);
                    }
                    None => {
                        return Err(anyhow!(
                            "Variable ({}) must be set to either a string value, \
                            or a mapping with one or more of 'default' and 'env' values.",
                            key
                        ));
                    }
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
                    .map(|(k, v)| {
                        let group = TestStepGroup::from_spec(&k, v, path);
                        (k, group)
                    })
                    .collect(),
            )
        }

        let mut vars: HashMap<String, String> = HashMap::new();
        let mut urls: HashMap<String, String> = HashMap::new();

        if let Some(spec_vars) = spec.vars {
            vars = ConfigData::create_variables(spec_vars)
                .map_err(|e| anyhow!("Error Decoding Config {}:\n{}", path.display(), e))?;
        }

        if let Some(spec_urls) = spec.urls {
            urls = ConfigData::create_variables(spec_urls)
                .map_err(|e| anyhow!("Error Decoding Config {}:\n{}", path.display(), e))?;
        }

        Ok(ConfigData {
            path: path.clone(),
            parent: None,
            step_sets,
            vars,
            urls,
        })
    }

    pub fn spec_from_val(value: Value) -> anyhow::Result<ConfigSpec> {
        from_value::<ConfigSpec>(value).map_err(|e| anyhow!("{}", e))
    }

    pub fn spec_from_file(path: &PathBuf) -> Result<ConfigSpec> {
        let file = File::open(path)
            .map_err(|_| anyhow!("Error Reading Config File: {}", path.display()))?;
        let reader = BufReader::new(file);
        serde_yaml::from_reader::<_, Value>(reader)
            .map_err(|e| anyhow!(e))
            .and_then(ConfigData::spec_from_val)
    }

    pub fn from_val(value: Value, path: &PathBuf) -> Result<ConfigData> {
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
        let cfg = config
            .as_ref()
            .ok_or_else(|| anyhow!("No config available to resolve step group '{}'", self.id))?;
        let step_group = cfg.read().unwrap().get_step_group(&self.id)?;
        step_group.run(config, prior_steps).await
    }
}

lazy_static! {
    static ref GROUP_TEST_RESULTS: DashMap<String, TestStepResult> = DashMap::new();
    static ref GROUP_INIT: DashMap<String, Arc<tokio::sync::Mutex<()>>> = DashMap::new();
    static ref GROUP_ONCE: DashMap<String, Arc<OnceCell<TestStepResult>>> = DashMap::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_path() -> PathBuf {
        PathBuf::from("/test/config.yaml")
    }

    fn make_config(yaml: &str) -> ConfigData {
        let val: Value = serde_yaml::from_str(yaml).unwrap();
        ConfigData::from_val(val, &test_path()).unwrap()
    }

    // ── step-set output resolution ───────────────────────────────────────────

    #[tokio::test]
    async fn test_step_set_args_resolve_in_output() {
        // With no steps, an output referencing `$args.email` must resolve from
        // the injected args scope.
        let cfg = make_config(
            "step-sets:\n  echo:\n    once: false\n    output:\n      who: $args.email\n    steps: []",
        );
        let group = cfg.get_step_group("echo").unwrap();
        let args = serde_json::json!({"email": "alice@example.com"});
        let result = group
            .run_with_args(&None, &HashMap::new(), Some(args))
            .await
            .unwrap();
        assert_eq!(
            result.get_field("who").unwrap(),
            Some(serde_json::json!("alice@example.com"))
        );
    }

    #[tokio::test]
    async fn test_step_set_literal_output_resolves() {
        // A literal (non-`$`) output value is resolved via the interpolation
        // engine; the old `$`-only loop skipped it entirely.
        let cfg = make_config(
            "step-sets:\n  greet:\n    once: false\n    output:\n      greeting: hello\n    steps: []",
        );
        let group = cfg.get_step_group("greet").unwrap();
        let result = group
            .run_internal(&None, &HashMap::new(), None)
            .await
            .unwrap();
        assert_eq!(result.get_field("greeting").unwrap(), Some(serde_json::json!("hello")));
    }

    // ── get_string_value ─────────────────────────────────────────────────────

    #[test]
    fn test_get_string_value_var() {
        let cfg = make_config("vars:\n  username: alice");
        assert_eq!(cfg.get_string_value("vars.username").unwrap(), "alice");
    }

    #[test]
    fn test_get_string_value_url() {
        let cfg = make_config("urls:\n  base: https://example.com");
        assert_eq!(cfg.get_string_value("urls.base").unwrap(), "https://example.com");
    }

    #[test]
    fn test_get_string_value_missing_key_errors() {
        let cfg = make_config("vars:\n  username: alice");
        assert!(cfg.get_string_value("vars.nonexistent").is_err());
    }

    #[test]
    fn test_get_string_value_walks_parent_chain() {
        let parent = make_config("vars:\n  parent_var: from_parent");
        let mut child = make_config("vars:\n  child_var: from_child");
        child.set_parent(Arc::new(RwLock::new(parent)));

        assert_eq!(child.get_string_value("vars.child_var").unwrap(), "from_child");
        assert_eq!(child.get_string_value("vars.parent_var").unwrap(), "from_parent");
    }

    #[test]
    fn test_get_string_value_chained_url_reference() {
        let cfg = make_config("urls:\n  base: https://example.com\n  api: \"$urls.base\"");
        assert_eq!(cfg.get_string_value("urls.api").unwrap(), "https://example.com");
    }

    // ── create_variables (via from_val) ──────────────────────────────────────

    #[test]
    fn test_variable_env_mapping_reads_env() {
        // SAFETY: single-threaded test process; no concurrent env access
        unsafe { std::env::set_var("YAPITEST_TEST_TOKEN", "secret_env_value") };
        let cfg = make_config("vars:\n  token:\n    env: YAPITEST_TEST_TOKEN");
        assert_eq!(cfg.get_string_value("vars.token").unwrap(), "secret_env_value");
        unsafe { std::env::remove_var("YAPITEST_TEST_TOKEN") };
    }

    #[test]
    fn test_variable_env_mapping_falls_back_to_default() {
        unsafe { std::env::remove_var("YAPITEST_TEST_UNSET_VAR") };
        let cfg = make_config(
            "vars:\n  token:\n    env: YAPITEST_TEST_UNSET_VAR\n    default: fallback",
        );
        assert_eq!(cfg.get_string_value("vars.token").unwrap(), "fallback");
    }

    #[test]
    fn test_variable_env_mapping_env_overrides_default() {
        unsafe { std::env::set_var("YAPITEST_TEST_BOTH", "env_wins") };
        let cfg = make_config(
            "vars:\n  token:\n    env: YAPITEST_TEST_BOTH\n    default: default_val",
        );
        assert_eq!(cfg.get_string_value("vars.token").unwrap(), "env_wins");
        unsafe { std::env::remove_var("YAPITEST_TEST_BOTH") };
    }

    #[test]
    fn test_variable_env_mapping_missing_env_no_default_errors() {
        unsafe { std::env::remove_var("YAPITEST_TEST_MISSING") };
        let val: Value =
            serde_yaml::from_str("vars:\n  token:\n    env: YAPITEST_TEST_MISSING")
                .unwrap();
        let result = ConfigData::from_val(val, &test_path());
        assert!(result.is_err());
        let msg = result.err().unwrap().to_string();
        assert!(msg.contains("token"), "{}", msg);
    }

    // ── get_step_group ───────────────────────────────────────────────────────

    #[test]
    fn test_get_step_group_not_found_errors() {
        let cfg = make_config("vars:\n  x: y");
        assert!(cfg.get_step_group("nonexistent").is_err());
    }

    #[test]
    fn test_get_step_group_found_directly() {
        let yaml = "step-sets:\n  my-setup:\n    once: false\n    output: {}\n    steps: []";
        let cfg = make_config(yaml);
        assert!(cfg.get_step_group("my-setup").is_ok());
    }

    #[test]
    fn test_get_step_group_found_in_parent() {
        let parent_yaml =
            "step-sets:\n  shared-setup:\n    once: false\n    output: {}\n    steps: []";
        let parent = make_config(parent_yaml);
        let mut child = make_config("vars:\n  x: y");
        child.set_parent(Arc::new(RwLock::new(parent)));
        assert!(child.get_step_group("shared-setup").is_ok());
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

        if !self.runs_once() {
            return self.run_internal(config, prior_steps, None).await;
        }

        if let Some(result) = GROUP_TEST_RESULTS.get(&test_group_id) {
            return Ok(result.value().clone());
        }

        let init_lock = GROUP_INIT
            .entry(test_group_id.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .value()
            .clone();

        let _guard = init_lock.lock().await;

        if let Some(result) = GROUP_TEST_RESULTS.get(&test_group_id) {
            return Ok(result.value().clone());
        }

        let result = self.run_internal(config, prior_steps, None).await?;

        GROUP_TEST_RESULTS.insert(test_group_id, result.clone());

        Ok(result)
    }
}
