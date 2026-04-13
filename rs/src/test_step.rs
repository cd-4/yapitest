use crate::config::ConfigData;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde::Deserialize;

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TestStepFailureReason {
    NoFailure,
    ResponseError,
    StatusCodeError,
    JsonDecodeError,
    ConfigurationError,
    SharedStepNotFoundError,
    Miscellaneous,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestStepAssertionSpec {
    status_code: Option<Value>,
    body: Option<Value>,
    full: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestStepSpec {
    id: Option<String>,
    path: String,
    url: Option<String>,
    method: Option<String>,
    headers: Option<HashMap<String, String>>,
    data: Option<Value>,
    assert: Option<TestStepAssertionSpec>,
}

pub struct TestStep {
    id: Option<String>,
    path: String,
    url: Option<String>,
    method: Method,
    header_data: HashMap<String, String>,
    request_data: Value,
    expected_response_data: Option<Value>,
    expected_status_code: Option<Value>,
    allow_missing_fields: bool,
}

#[derive(Debug, Clone)]
pub struct TestStepResult {
    step_id: Option<String>,
    pub response_data: Option<Value>,
    pub request_data: Option<Value>,
    pub output_data: Option<Value>,
    pub status: TestStepFailureReason,
    pub failure_message: Option<String>,
}

pub fn get_variable(
    name: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<Value> {
    if !name.starts_with('$') {
        return Ok(Value::String(name.to_owned()));
    }
    let mut current_key = name.to_owned();
    'outer: while current_key.starts_with('$') {
        let value_key = &current_key[1..];

        if let Some(cfg) = config {
            if let Ok(new_val) = cfg.read().unwrap().get_string_value(value_key) {
                current_key = new_val;
                if current_key.starts_with('$') {
                    continue 'outer;
                } else {
                    return Ok(Value::from(current_key));
                }
            }
        }

        let mut segments: Vec<&str> = value_key.split('.').collect();

        if let Some(step) = segments.first().copied().and_then(|v| prior_steps.get(v)) {
            let step_id = segments[0];
            segments.remove(0);
            let field_key = segments.join(".");
            match step.get_field(&field_key) {
                Ok(field_val) => {
                    if let Some(val) = field_val {
                        if let Some(value_str) = val.as_str() {
                            if value_str.starts_with('$') {
                                current_key = value_str.to_owned();
                                continue 'outer;
                            }
                        }
                        return Ok(val);
                    } else {
                        return Err(anyhow!(
                            "'{}' — '{}' not found in step '{}'",
                            name,
                            field_key,
                            step_id
                        ));
                    }
                }
                Err(_) => {
                    return Err(anyhow!(
                        "'{}' — '{}' not found in step '{}'",
                        name,
                        field_key,
                        step_id
                    ));
                }
            }
        }

        let step_id = segments.into_iter().next().unwrap_or_default();
        return Err(anyhow!(
            "'{}' — no step with id '{}' was found",
            name,
            step_id
        ));
    }
    Err(anyhow!("'{}' could not be resolved", name))
}

pub fn clean_request_data(
    request_data: &Value,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<Value> {
    if let Some(data_map) = request_data.as_object() {
        let mut new_value = Map::with_capacity(data_map.len());
        for (k, v) in data_map {
            new_value.insert(k.clone(), clean_request_data(v, config, prior_steps)?);
        }
        Ok(Value::Object(new_value))
    } else if let Some(data_arr) = request_data.as_array() {
        let mut new_val = Vec::with_capacity(data_arr.len());
        for item in data_arr {
            new_val.push(clean_request_data(item, config, prior_steps)?);
        }
        Ok(Value::Array(new_val))
    } else if let Some(data_str) = request_data.as_str() {
        if data_str.starts_with('$') {
            get_variable(data_str, config, prior_steps)
        } else {
            Ok(Value::from(data_str))
        }
    } else {
        Ok(request_data.clone())
    }
}

pub fn clean_path(
    path: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<String> {
    let ends_with_slash = path.ends_with('/');

    let mut segments: Vec<String> = vec![];

    for segment in path.split('/') {
        if segment.starts_with('$') {
            let new_seg = get_variable(segment, config, prior_steps)?;
            if let Some(seg_str) = new_seg.as_str() {
                segments.push(seg_str.to_owned());
            } else if let Some(seg_int) = new_seg.as_i64() {
                segments.push(format!("{}", seg_int));
            } else {
                return Err(anyhow!(
                    "path variable '{}' must resolve to a string or integer, got {}",
                    segment,
                    value_type_name(&new_seg)
                ));
            }
        } else {
            segments.push(segment.to_owned());
        }
    }
    let mut output = segments.join("/");
    output.insert(0, '/');
    if ends_with_slash {
        output.push('/');
    }
    Ok(output)
}

pub fn clean_headers(
    header_data: &HashMap<String, String>,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<HeaderMap> {
    let mut output: HeaderMap = HeaderMap::new();
    for (k, v) in header_data {
        if v.starts_with('$') {
            match get_variable(v, config, prior_steps) {
                Ok(header_value) => {
                    if let Some(header_str) = header_value.as_str() {
                        let name = HeaderName::from_bytes(k.as_bytes()).unwrap();
                        let val = HeaderValue::from_str(header_str).unwrap();
                        output.insert(name, val);
                    } else {
                        return Err(anyhow!(
                            "header '{}': '{}' resolved to a non-string value",
                            k,
                            v
                        ));
                    }
                }
                Err(e) => {
                    return Err(anyhow!("header '{}': could not resolve '{}' — {}", k, v, e));
                }
            }
        } else {
            let name = HeaderName::from_bytes(k.as_bytes()).unwrap();
            let val = HeaderValue::from_str(v).unwrap();
            output.insert(name, val);
        }
    }
    Ok(output)
}

#[derive(Debug, PartialEq)]
enum Operator {
    Gt,  // >
    Gte, // >=
    Lt,  // <
    Lte, // <=
    Eq,  // =
}

#[derive(Debug, PartialEq)]
struct Comparison {
    op: Operator,
    value: i64,
}

fn parse_comparison(s: &str) -> Result<Comparison, String> {
    let re = Regex::new(r"^\s*([<>]=?|=?)\s*(\d+)\s*$").map_err(|e| e.to_string())?;

    let caps = re
        .captures(s.trim())
        .ok_or_else(|| format!("Invalid comparison format: '{}'", s))?;

    let op_str = caps.get(1).unwrap().as_str();
    let num_str = caps.get(2).unwrap().as_str();

    let op = match op_str {
        ">" => Operator::Gt,
        ">=" => Operator::Gte,
        "<" => Operator::Lt,
        "<=" => Operator::Lte,
        "=" | "" => Operator::Eq,
        _ => return Err(format!("Unknown operator: {}", op_str)),
    };

    let value: i64 = num_str.parse::<i64>().map_err(|e| e.to_string())?;

    Ok(Comparison { op, value })
}

pub fn get_value_length(val: &Value) -> Result<i64> {
    if let Some(value_str) = val.as_str() {
        return Ok(value_str.len() as i64);
    } else if let Some(value_arr) = val.as_array() {
        return Ok(value_arr.len() as i64);
    } else if let Some(value_obj) = val.as_object() {
        return Ok(value_obj.len() as i64);
    }

    Err(anyhow!(
        "cannot check length of a {} value",
        value_type_name(val)
    ))
}

pub fn check_size(val: &Value, size_str: &str) -> Result<bool> {
    let value_size = get_value_length(val)?;

    match parse_comparison(size_str) {
        Ok(cmp) => match cmp.op {
            Operator::Gt => Ok(value_size > cmp.value),
            Operator::Lt => Ok(value_size < cmp.value),
            Operator::Eq => Ok(value_size == cmp.value),
            Operator::Gte => Ok(value_size >= cmp.value),
            Operator::Lte => Ok(value_size <= cmp.value),
        },
        Err(e) => Err(anyhow!("invalid size comparison '{}': {}", size_str, e)),
    }
}

pub fn compare_data_objects(
    observed_object: &Map<String, Value>,
    expected_object: &Map<String, Value>,
    full: bool,
    keys: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<()> {
    // Check every expected key is present and matches in the observed response.
    // Iterating expected (not observed) ensures missing fields are caught.
    for key in expected_object.keys() {
        let expected = expected_object.get(key).unwrap();

        // `len(field)` keys are size assertions handled in the observed pass below.
        if key.starts_with("len(") && key.ends_with(')') {
            continue;
        }

        let observed = match observed_object.get(key) {
            Some(v) => v,
            None => {
                let path = if keys.is_empty() {
                    key.as_str()
                } else {
                    &format!("{}.{}", keys.trim_start_matches('.'), key)
                };
                return Err(anyhow!("missing field '{}' in response", path));
            }
        };

        let new_keys = format!("{}.{}", keys, key);
        compare_data_inner(observed, expected, full, &new_keys, config, prior_steps)?;
    }

    // Walk observed keys for `len(field)` size checks and the `full` mode check
    // (no unexpected extra fields in the response).
    for key in observed_object.keys() {
        let observed = observed_object.get(key).unwrap();

        let size_key = format!("len({})", key);
        if let Some(expected_size) = expected_object.get(&size_key) {
            let cmp_str = expected_size.as_str().unwrap();
            let field_path = if keys.is_empty() {
                key.as_str()
            } else {
                &format!("{}.{}", keys.trim_start_matches('.'), key)
            };
            match get_value_length(observed) {
                Ok(actual_len) => match check_size(observed, cmp_str) {
                    Ok(true) => {}
                    Ok(false) => {
                        return Err(anyhow!(
                            "len({}) expected {}, got {}",
                            field_path,
                            cmp_str,
                            actual_len
                        ));
                    }
                    Err(_) => {
                        return Err(anyhow!(
                            "invalid size comparison '{}' on field '{}'",
                            cmp_str,
                            field_path
                        ));
                    }
                },
                Err(e) => return Err(e),
            }
        } else if full && !expected_object.contains_key(key) {
            let field_path = if keys.is_empty() {
                key.as_str()
            } else {
                &format!("{}.{}", keys.trim_start_matches('.'), key)
            };
            return Err(anyhow!(
                "unexpected field '{}' in response — add it to the 'body' assertion or remove 'full: true'",
                field_path
            ));
        }
    }

    Ok(())
}

pub fn compare_array_objects(
    observed_object: &[Value],
    expected_object: &[Value],
    full: bool,
    keys: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<()> {
    let num_expected = expected_object.len();
    let num_observed = observed_object.len();
    if num_expected != num_observed {
        let path = keys.trim_start_matches('.');
        return Err(anyhow!(
            "'{}' — expected {} item(s), got {}",
            path,
            num_expected,
            num_observed
        ));
    }

    for (index, (observed, expected)) in observed_object.iter().zip(expected_object.iter()).enumerate() {
        let new_keys = format!("{}.[{}]", keys, index);
        compare_data_inner(observed, expected, full, &new_keys, config, prior_steps)?;
    }

    Ok(())
}

fn value_type_name(v: &Value) -> &'static str {
    if v.is_null() {
        "Null"
    } else if v.is_boolean() {
        "Bool"
    } else if v.is_number() {
        "Number"
    } else if v.is_string() {
        "String"
    } else if v.is_array() {
        "Array"
    } else if v.is_object() {
        "Object"
    } else {
        "Unknown"
    }
}

fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(xa, ya)| value_eq(xa, ya))
        }
        (Value::Object(x), Value::Object(y)) => {
            if x.len() != y.len() {
                return false;
            }
            x.iter()
                .all(|(k, v)| y.get(k).map_or(false, |yv| value_eq(v, yv)))
        }
        _ => false,
    }
}

pub fn compare_primitive_values(
    observed: &Value,
    expected: &Value,
    keys: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<()> {
    let path = keys.trim_start_matches('.');

    if let Some(exp_str) = expected.as_str() {
        if exp_str.starts_with('+') {
            let exp_type = &exp_str[1..];
            let type_ok = match exp_type {
                "str" | "string" => observed.as_str().is_some(),
                "float" | "flt" => observed.as_f64().is_some(),
                "int" | "integer" => observed.as_i64().is_some(),
                "bool" | "boolean" => observed.as_bool().is_some(),
                "arr" | "array" | "list" => observed.as_array().is_some(),
                "dict" | "dic" | "dictionary" | "obj" | "object" | "map" => {
                    observed.as_object().is_some()
                }
                _ => true,
            };
            if !type_ok {
                let readable_type = match exp_type {
                    "str" | "string" => "a string",
                    "float" | "flt" => "a float",
                    "int" | "integer" => "an integer",
                    "bool" | "boolean" => "a boolean",
                    "arr" | "array" | "list" => "an array",
                    _ => "an object",
                };
                return Err(anyhow!(
                    "'{}' — expected {}, got {} ({})",
                    path,
                    readable_type,
                    value_type_name(observed),
                    observed
                ));
            }
            return Ok(());
        } else if exp_str.starts_with('$') {
            let exp_var = get_variable(exp_str, config, prior_steps)?;
            if !value_eq(&exp_var, observed) {
                return Err(anyhow!(
                    "'{}' — expected {}, got {}",
                    path,
                    exp_var,
                    observed
                ));
            } else {
                return Ok(());
            }
        }
    }

    if value_type_name(observed) != value_type_name(expected) {
        return Err(anyhow!(
            "'{}' — expected {} ({}), got {} ({})",
            path,
            value_type_name(expected),
            expected,
            value_type_name(observed),
            observed,
        ));
    }

    if !value_eq(observed, expected) {
        Err(anyhow!(
            "'{}' — expected {}, got {}",
            path,
            expected,
            observed
        ))
    } else {
        Ok(())
    }
}

pub fn compare_data_inner(
    observed: &Value,
    expected: &Value,
    full: bool,
    keys: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<()> {
    if let (Some(obs_obj), Some(exp_obj)) = (observed.as_object(), expected.as_object()) {
        compare_data_objects(obs_obj, exp_obj, full, keys, config, prior_steps)
    } else if let (Some(obs_arr), Some(exp_arr)) = (observed.as_array(), expected.as_array()) {
        compare_array_objects(obs_arr, exp_arr, full, keys, config, prior_steps)
    } else {
        compare_primitive_values(observed, expected, keys, config, prior_steps)
    }
}

pub fn compare_data(
    observed: &Value,
    expected: &Value,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
    full: bool,
) -> Result<()> {
    compare_data_inner(observed, expected, full, "", config, prior_steps)
}

impl TestStepResult {
    pub fn make_failure(
        step_id: Option<&str>,
        reason: TestStepFailureReason,
        message: String,
    ) -> TestStepResult {
        TestStepResult {
            step_id: step_id.map(str::to_owned),
            status: reason,
            response_data: None,
            request_data: None,
            output_data: None,
            failure_message: Some(message),
        }
    }

    pub fn make_success(
        step_id: Option<&str>,
        response_data: Value,
        request_data: Value,
        output_data: Value,
    ) -> TestStepResult {
        TestStepResult {
            step_id: step_id.map(str::to_owned),
            status: TestStepFailureReason::NoFailure,
            response_data: Some(response_data),
            request_data: Some(request_data),
            output_data: Some(output_data),
            failure_message: None,
        }
    }

    pub fn get_field(&self, keys: &str) -> Result<Option<Value>> {
        let sections: Vec<&str> = keys.split('.').collect();
        let mut current: Option<&Value> = None;
        let mut first = true;

        // Step group results store outputs directly; skip namespace routing.
        if let Some(output) = &self.output_data {
            current = Some(output);
            first = false;
        }

        for section in &sections {
            if first {
                current = match *section {
                    "response" => self.response_data.as_ref(),
                    "request" | "data" => self.request_data.as_ref(),
                    "output" => self.output_data.as_ref(),
                    _ => return Err(anyhow!("Section {} not found in step", section)),
                };
                first = false;
            } else {
                current = current
                    .and_then(|v| v.as_object())
                    .and_then(|obj| obj.get(*section));
            }
        }
        Ok(current.cloned())
    }
}

impl TestStep {
    fn check_status_code(exp: &Value, actual: u16) -> bool {
        if let Some(int_val) = exp.as_u64() {
            return int_val == u64::from(actual);
        }
        if let Some(exp_str) = exp.as_str() {
            let act_str = actual.to_string();
            if exp_str.len() != act_str.len() {
                return false;
            }
            return exp_str
                .chars()
                .zip(act_str.chars())
                .all(|(exp_char, act_char)| exp_char == 'x' || exp_char == act_char);
        }
        false
    }

    fn get_url(&self, config: &Option<Arc<RwLock<ConfigData>>>) -> Result<String> {
        if let Some(url_val) = &self.url {
            if url_val.starts_with('$') {
                if let Some(cfg) = config {
                    return cfg.read().unwrap().get_string_value(&url_val[1..]);
                }
            } else {
                return Ok(url_val.clone());
            }
        }
        if let Some(cfg) = config {
            return cfg.read().unwrap().get_string_value("urls.base");
        }
        Err(anyhow!("Url not found"))
    }

    fn get_method(method_str: Option<String>) -> Method {
        if let Some(method) = method_str {
            let upper_method = method.to_uppercase();
            match Method::from_str(&upper_method) {
                Ok(method_enum) => method_enum,
                Err(e) => {
                    panic!("Invalid Method {}", e);
                }
            }
        } else {
            Method::GET
        }
    }

    pub fn from_spec(spec: TestStepSpec) -> TestStep {
        let mut header_data: HashMap<String, String> = HashMap::new();
        if let Some(headers) = spec.headers {
            header_data = headers;
        }

        let mut req_data: Value = Value::Null;
        if let Some(request_data) = spec.data {
            req_data = request_data;
        }

        let mut expected_response_data: Option<Value> = None;
        let mut expected_status_code: Option<Value> = None;
        let mut full_data: bool = false;
        if let Some(assertion_data) = spec.assert {
            expected_response_data = assertion_data.body;
            expected_status_code = assertion_data.status_code;
            if let Some(full) = assertion_data.full {
                full_data = full;
            }
        }

        TestStep {
            id: spec.id,
            url: spec.url,
            path: spec.path,
            method: TestStep::get_method(spec.method),
            header_data,
            request_data: req_data,
            expected_response_data,
            expected_status_code,
            allow_missing_fields: !full_data,
        }
    }
}

#[async_trait]
pub trait RunnableTestStep {
    fn get_id(&self) -> Option<&String>;
    async fn run(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> Result<TestStepResult>;
}

#[async_trait]
impl RunnableTestStep for TestStep {
    fn get_id(&self) -> Option<&String> {
        self.id.as_ref()
    }

    async fn run(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> Result<TestStepResult> {
        let client = Client::new();

        let mut url = match self.get_url(config) {
            Ok(actual_url) => actual_url,
            Err(_) => {
                return Ok(TestStepResult::make_failure(
                    self.id.as_deref(),
                    TestStepFailureReason::ConfigurationError,
                    "no base URL configured — set 'urls.base' in a config file".to_string(),
                ));
            }
        };

        if url.ends_with('/') {
            url.pop();
        }

        // Avoid cloning path when it already has the leading slash.
        let path_owned;
        let path: &str = if self.path.starts_with('/') {
            &self.path
        } else {
            path_owned = format!("/{}", self.path);
            &path_owned
        };

        let path = match clean_path(path, config, prior_steps) {
            Ok(p) => p,
            Err(e) => return Err(anyhow!("could not build request path: {}", e)),
        };

        let full_url = format!("{}{}", url, path);

        let headers = clean_headers(&self.header_data, config, prior_steps)?;
        let req_data = clean_request_data(&self.request_data, config, prior_steps)?;
        let mut response_data: Option<Value> = None;

        match client
            .request(self.method.clone(), full_url)
            .headers(headers)
            .json(&req_data)
            .send()
            .await
        {
            Ok(response) => {
                if let Some(exp_status_code) = &self.expected_status_code {
                    let actual_status_code = response.status().as_u16();
                    if !TestStep::check_status_code(exp_status_code, actual_status_code) {
                        let failure_message = format!(
                            "expected status {}, got {}",
                            exp_status_code, actual_status_code,
                        );
                        return Ok(TestStepResult::make_failure(
                            self.id.as_deref(),
                            TestStepFailureReason::StatusCodeError,
                            failure_message,
                        ));
                    }
                }

                let res_text = response.text().await?;

                match serde_json::from_str::<Value>(&res_text) {
                    Ok(actual_response) => {
                        if let Some(expected_response) = &self.expected_response_data {
                            if let Err(e) = compare_data(
                                &actual_response,
                                expected_response,
                                config,
                                prior_steps,
                                !self.allow_missing_fields,
                            ) {
                                return Ok(TestStepResult::make_failure(
                                    self.id.as_deref(),
                                    TestStepFailureReason::ResponseError,
                                    format!("{}", e),
                                ));
                            }
                        }
                        // Move actual_response rather than cloning it.
                        response_data = Some(actual_response);
                    }
                    Err(e) => {
                        if self.expected_response_data.is_some() {
                            return Ok(TestStepResult::make_failure(
                                self.id.as_deref(),
                                TestStepFailureReason::JsonDecodeError,
                                format!("response body is not valid JSON: {}", e),
                            ));
                        }
                    }
                }
            }
            Err(e) => {
                return Err(anyhow!("HTTP request failed: {}", e));
            }
        }

        Ok(TestStepResult {
            step_id: self.id.clone(),
            status: TestStepFailureReason::NoFailure,
            failure_message: None,
            request_data: Some(req_data),
            response_data,
            output_data: None,
        })
    }
}
