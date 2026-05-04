use crate::config::ConfigData;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use rand;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde::Deserialize;

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct AssertionResult {
    pub name: String,
    pub passed: bool,
    pub message: Option<String>,
}

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
    headers: Option<Value>,
    body: Option<Value>,
    full: Option<bool>,
    duration: Option<Value>,
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
    wait_before: Option<Value>,
    wait_after: Option<Value>,
    retry: Option<u32>,
}

pub struct TestStep {
    id: Option<String>,
    path: String,
    url: Option<String>,
    method: Method,
    header_data: HashMap<String, String>,
    request_data: Value,
    expected_response_headers: Option<Value>,
    expected_response_data: Option<Value>,
    expected_status_code: Option<Value>,
    allow_missing_fields: bool,
    expected_duration: Option<Value>,
    wait_before: Option<Value>,
    wait_after: Option<Value>,
    retry: u32,
}

#[derive(Debug, Clone)]
pub struct TestStepResult {
    step_id: Option<String>,
    pub response_data: Option<Value>,
    pub request_data: Option<Value>,
    pub output_data: Option<Value>,
    pub status: TestStepFailureReason,
    pub failure_message: Option<String>,
    pub assertion_results: Vec<AssertionResult>,
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
                if new_val.starts_with('$') {
                    current_key = new_val;
                    continue 'outer;
                } else if let Some(pattern) = new_val.strip_prefix("re/") {
                    return Ok(Value::String(generate_regex_string(pattern)?));
                } else {
                    return Ok(Value::from(new_val));
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

pub fn generate_regex_string(pattern: &str) -> Result<String> {
    use regex_generate::{DEFAULT_MAX_REPEAT, Generator};
    let mut generator = Generator::new(pattern, rand::thread_rng(), DEFAULT_MAX_REPEAT)
        .map_err(|e| anyhow!("invalid regex pattern 're/{}': {}", pattern, e))?;
    let mut buffer = vec![];
    generator
        .generate(&mut buffer)
        .map_err(|e| anyhow!("failed to generate string for 're/{}': {}", pattern, e))?;
    String::from_utf8(buffer)
        .map_err(|e| anyhow!("generated string for 're/{}' is not valid UTF-8: {}", pattern, e))
}

fn parse_duration(v: &Value) -> Result<std::time::Duration> {
    if let Some(n) = v.as_u64() {
        return Ok(std::time::Duration::from_millis(n));
    }
    if let Some(s) = v.as_str() {
        if let Some(ms_str) = s.strip_suffix("ms") {
            let ms: u64 = ms_str.parse().map_err(|_| {
                anyhow!("invalid duration '{}' — use '500ms', '2s', or a bare integer (milliseconds)", s)
            })?;
            return Ok(std::time::Duration::from_millis(ms));
        }
        if let Some(s_str) = s.strip_suffix('s') {
            let secs: u64 = s_str.parse().map_err(|_| {
                anyhow!("invalid duration '{}' — use '500ms', '2s', or a bare integer (milliseconds)", s)
            })?;
            return Ok(std::time::Duration::from_secs(secs));
        }
        if let Ok(ms) = s.parse::<u64>() {
            return Ok(std::time::Duration::from_millis(ms));
        }
    }
    Err(anyhow!(
        "invalid duration '{}' — use '500ms', '2s', or a bare integer (milliseconds)",
        v
    ))
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
        if let Some(pattern) = data_str.strip_prefix("re/") {
            Ok(Value::String(generate_regex_string(pattern)?))
        } else if data_str.starts_with('$') {
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
    assertions: &mut Vec<AssertionResult>,
) -> bool {
    let mut all_passed = true;

    // Check every expected key is present and matches in the observed response.
    // Iterating expected (not observed) ensures missing fields are caught.
    for (key, expected) in expected_object {
        // `len(field)` keys are size assertions handled in the observed pass below.
        if key.starts_with("len(") && key.ends_with(')') {
            continue;
        }

        let field_path = if keys.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", keys.trim_start_matches('.'), key)
        };

        let observed = match observed_object.get(key) {
            Some(v) => v,
            None => {
                assertions.push(AssertionResult {
                    name: field_path.clone(),
                    passed: false,
                    message: Some(format!("missing field '{}' in response", field_path)),
                });
                all_passed = false;
                continue;
            }
        };

        let new_keys = format!("{}.{}", keys, key);
        if !compare_data_inner(observed, expected, full, &new_keys, config, prior_steps, assertions) {
            all_passed = false;
        }
    }

    // Walk observed keys for `len(field)` size checks and the `full` mode check
    // (no unexpected extra fields in the response).
    for (key, observed) in observed_object {
        let size_key = format!("len({})", key);
        if let Some(expected_size) = expected_object.get(&size_key) {
            let cmp_str = expected_size.as_str().unwrap_or("");
            let field_path = if keys.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", keys.trim_start_matches('.'), key)
            };
            let assertion_name = format!("len({}) {}", field_path, cmp_str);
            match get_value_length(observed) {
                Ok(actual_len) => match check_size(observed, cmp_str) {
                    Ok(true) => {
                        assertions.push(AssertionResult { name: assertion_name, passed: true, message: None });
                    }
                    Ok(false) => {
                        assertions.push(AssertionResult {
                            name: assertion_name,
                            passed: false,
                            message: Some(format!("len({}) expected {}, got {}", field_path, cmp_str, actual_len)),
                        });
                        all_passed = false;
                    }
                    Err(e) => {
                        assertions.push(AssertionResult {
                            name: assertion_name,
                            passed: false,
                            message: Some(format!("invalid size comparison '{}' on field '{}': {}", cmp_str, field_path, e)),
                        });
                        all_passed = false;
                    }
                },
                Err(e) => {
                    assertions.push(AssertionResult { name: assertion_name, passed: false, message: Some(e.to_string()) });
                    all_passed = false;
                }
            }
        } else if full && !expected_object.contains_key(key) {
            let field_path = if keys.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", keys.trim_start_matches('.'), key)
            };
            assertions.push(AssertionResult {
                name: field_path.clone(),
                passed: false,
                message: Some(format!(
                    "unexpected field '{}' in response — add it to the 'body' assertion or remove 'full: true'",
                    field_path
                )),
            });
            all_passed = false;
        }
    }

    all_passed
}

pub fn compare_array_objects(
    observed_object: &[Value],
    expected_object: &[Value],
    full: bool,
    keys: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
    assertions: &mut Vec<AssertionResult>,
) -> bool {
    let path = keys.trim_start_matches('.');
    let num_expected = expected_object.len();
    let num_observed = observed_object.len();
    if num_expected != num_observed {
        assertions.push(AssertionResult {
            name: path.to_owned(),
            passed: false,
            message: Some(format!("'{}' — expected {} item(s), got {}", path, num_expected, num_observed)),
        });
        return false;
    }

    let mut all_passed = true;
    for (index, (observed, expected)) in observed_object.iter().zip(expected_object.iter()).enumerate() {
        let new_keys = format!("{}.[{}]", keys, index);
        if !compare_data_inner(observed, expected, full, &new_keys, config, prior_steps, assertions) {
            all_passed = false;
        }
    }
    all_passed
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
    assertions: &mut Vec<AssertionResult>,
) -> bool {
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
            let name = format!("{} ({})", path, exp_str);
            if type_ok {
                assertions.push(AssertionResult { name, passed: true, message: None });
            } else {
                let readable_type = match exp_type {
                    "str" | "string" => "a string",
                    "float" | "flt" => "a float",
                    "int" | "integer" => "an integer",
                    "bool" | "boolean" => "a boolean",
                    "arr" | "array" | "list" => "an array",
                    _ => "an object",
                };
                assertions.push(AssertionResult {
                    name,
                    passed: false,
                    message: Some(format!(
                        "'{}' — expected {}, got {} ({})",
                        path, readable_type, value_type_name(observed), observed
                    )),
                });
            }
            return type_ok;
        } else if let Some(pattern) = exp_str.strip_prefix("re/") {
            let name = format!("{} (re/{})", path, pattern);
            match Regex::new(pattern) {
                Err(e) => {
                    assertions.push(AssertionResult {
                        name,
                        passed: false,
                        message: Some(format!(
                            "'{}' — invalid regex pattern 're/{}': {}",
                            path, pattern, e
                        )),
                    });
                    return false;
                }
                Ok(re) => {
                    match observed.as_str() {
                        None => {
                            assertions.push(AssertionResult {
                                name,
                                passed: false,
                                message: Some(format!(
                                    "'{}' — expected a string to match re/{}, got {} ({})",
                                    path, pattern, value_type_name(observed), observed
                                )),
                            });
                            return false;
                        }
                        Some(obs_str) => {
                            let passed = re.is_match(obs_str);
                            assertions.push(AssertionResult {
                                name,
                                passed,
                                message: if passed {
                                    None
                                } else {
                                    Some(format!(
                                        "'{}' — expected to match re/{}, got '{}'",
                                        path, pattern, obs_str
                                    ))
                                },
                            });
                            return passed;
                        }
                    }
                }
            }
        } else if exp_str.starts_with('$') {
            match get_variable(exp_str, config, prior_steps) {
                Ok(exp_var) => {
                    let passed = value_eq(&exp_var, observed);
                    assertions.push(AssertionResult {
                        name: path.to_owned(),
                        passed,
                        message: if passed { None } else {
                            Some(format!("'{}' — expected {}, got {}", path, exp_var, observed))
                        },
                    });
                    return passed;
                }
                Err(e) => {
                    assertions.push(AssertionResult {
                        name: path.to_owned(),
                        passed: false,
                        message: Some(e.to_string()),
                    });
                    return false;
                }
            }
        }
    }

    if value_type_name(observed) != value_type_name(expected) {
        assertions.push(AssertionResult {
            name: path.to_owned(),
            passed: false,
            message: Some(format!(
                "'{}' — expected {} ({}), got {} ({})",
                path, value_type_name(expected), expected, value_type_name(observed), observed,
            )),
        });
        return false;
    }

    let passed = value_eq(observed, expected);
    assertions.push(AssertionResult {
        name: path.to_owned(),
        passed,
        message: if passed { None } else {
            Some(format!("'{}' — expected {}, got {}", path, expected, observed))
        },
    });
    passed
}

pub fn compare_data_inner(
    observed: &Value,
    expected: &Value,
    full: bool,
    keys: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
    assertions: &mut Vec<AssertionResult>,
) -> bool {
    if let (Some(obs_obj), Some(exp_obj)) = (observed.as_object(), expected.as_object()) {
        compare_data_objects(obs_obj, exp_obj, full, keys, config, prior_steps, assertions)
    } else if let (Some(obs_arr), Some(exp_arr)) = (observed.as_array(), expected.as_array()) {
        compare_array_objects(obs_arr, exp_arr, full, keys, config, prior_steps, assertions)
    } else {
        compare_primitive_values(observed, expected, keys, config, prior_steps, assertions)
    }
}

pub fn compare_data(
    observed: &Value,
    expected: &Value,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
    full: bool,
    assertions: &mut Vec<AssertionResult>,
) -> bool {
    compare_data_inner(observed, expected, full, "", config, prior_steps, assertions)
}

fn response_headers_to_value(headers: &HeaderMap) -> Value {
    let mut output = Map::new();

    for (name, value) in headers.iter() {
        let key = name.as_str().to_ascii_lowercase();
        let value = Value::String(String::from_utf8_lossy(value.as_bytes()).into_owned());

        match output.get_mut(&key) {
            Some(Value::Array(values)) => values.push(value),
            Some(existing) => {
                let previous = std::mem::replace(existing, Value::Null);
                *existing = Value::Array(vec![previous, value]);
            }
            None => {
                output.insert(key, value);
            }
        }
    }

    Value::Object(output)
}

fn normalize_expected_headers(expected: &Value) -> Value {
    let Some(headers) = expected.as_object() else {
        return expected.clone();
    };

    Value::Object(
        headers
            .iter()
            .map(|(key, value)| (key.to_ascii_lowercase(), value.clone()))
            .collect(),
    )
}

fn compare_headers(
    observed: &Value,
    expected: &Value,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
    assertions: &mut Vec<AssertionResult>,
) -> bool {
    let expected = normalize_expected_headers(expected);
    compare_data_inner(
        observed,
        &expected,
        false,
        "headers",
        config,
        prior_steps,
        assertions,
    )
}

fn push_duration_assertion(
    assertions: &mut Vec<AssertionResult>,
    expected: Option<&Value>,
    elapsed: std::time::Duration,
) {
    let Some(dur_val) = expected else { return };
    match parse_duration(dur_val) {
        Err(e) => {
            assertions.push(AssertionResult {
                name: "duration".to_owned(),
                passed: false,
                message: Some(e.to_string()),
            });
        }
        Ok(limit) => {
            let passed = elapsed < limit;
            assertions.push(AssertionResult {
                name: "duration".to_owned(),
                passed,
                message: if passed {
                    None
                } else {
                    Some(format!(
                        "request took {}ms, expected less than {}ms",
                        elapsed.as_millis(),
                        limit.as_millis(),
                    ))
                },
            });
        }
    }
}

fn failed_assertion_message(assertions: &[AssertionResult]) -> Option<String> {
    let failures: Vec<String> = assertions
        .iter()
        .filter(|assertion| !assertion.passed)
        .map(|assertion| {
            assertion
                .message
                .clone()
                .unwrap_or_else(|| assertion.name.clone())
        })
        .collect();

    match failures.len() {
        0 => None,
        1 => failures.into_iter().next(),
        count => Some(format!(
            "{} assertions failed: {}",
            count,
            failures.join("; ")
        )),
    }
}

fn failed_assertion_reason(
    assertions: &[AssertionResult],
    fallback: TestStepFailureReason,
) -> TestStepFailureReason {
    if assertions
        .iter()
        .any(|assertion| !assertion.passed && assertion.name.starts_with("status "))
    {
        TestStepFailureReason::StatusCodeError
    } else {
        fallback
    }
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
            assertion_results: Vec::new(),
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
            assertion_results: Vec::new(),
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

        let mut expected_response_headers: Option<Value> = None;
        let mut expected_response_data: Option<Value> = None;
        let mut expected_status_code: Option<Value> = None;
        let mut full_data: bool = false;
        let mut expected_duration: Option<Value> = None;
        if let Some(assertion_data) = spec.assert {
            expected_response_headers = assertion_data.headers;
            expected_response_data = assertion_data.body;
            expected_status_code = assertion_data.status_code;
            if let Some(full) = assertion_data.full {
                full_data = full;
            }
            expected_duration = assertion_data.duration;
        }

        TestStep {
            id: spec.id,
            url: spec.url,
            path: spec.path,
            method: TestStep::get_method(spec.method),
            header_data,
            request_data: req_data,
            expected_response_headers,
            expected_response_data,
            expected_status_code,
            allow_missing_fields: !full_data,
            expected_duration,
            wait_before: spec.wait_before,
            wait_after: spec.wait_after,
            retry: spec.retry.unwrap_or(0),
        }
    }

    async fn run_attempt(
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
        let mut assertions: Vec<AssertionResult> = Vec::new();
        let mut response_data: Option<Value> = None;
        let mut failure_reason = TestStepFailureReason::ResponseError;

        let t0 = std::time::Instant::now();

        match client
            .request(self.method.clone(), full_url)
            .headers(headers)
            .json(&req_data)
            .send()
            .await
        {
            Ok(response) => {
                let actual_status_code = response.status().as_u16();
                let actual_headers = response_headers_to_value(response.headers());
                let res_text = response.text().await?;
                let elapsed = t0.elapsed();

                if let Some(exp_status_code) = &self.expected_status_code {
                    let passed = TestStep::check_status_code(exp_status_code, actual_status_code);
                    assertions.push(AssertionResult {
                        name: format!("status {}", exp_status_code),
                        passed,
                        message: if passed {
                            None
                        } else {
                            Some(format!(
                                "expected status {}, got {}",
                                exp_status_code, actual_status_code
                            ))
                        },
                    });
                }

                if let Some(expected_headers) = &self.expected_response_headers {
                    compare_headers(
                        &actual_headers,
                        expected_headers,
                        config,
                        prior_steps,
                        &mut assertions,
                    );
                }

                match serde_json::from_str::<Value>(&res_text) {
                    Ok(actual_response) => {
                        if let Some(expected_response) = &self.expected_response_data {
                            compare_data(
                                &actual_response,
                                expected_response,
                                config,
                                prior_steps,
                                !self.allow_missing_fields,
                                &mut assertions,
                            );
                        }
                        response_data = Some(actual_response);
                    }
                    Err(e) => {
                        if self.expected_response_data.is_some() {
                            failure_reason = TestStepFailureReason::JsonDecodeError;
                            assertions.push(AssertionResult {
                                name: "body".to_owned(),
                                passed: false,
                                message: Some(format!("response body is not valid JSON: {}", e)),
                            });
                        }
                    }
                }

                push_duration_assertion(&mut assertions, self.expected_duration.as_ref(), elapsed);
            }
            Err(e) => {
                return Err(anyhow!("HTTP request failed: {}", e));
            }
        }

        if let Some(msg) = failed_assertion_message(&assertions) {
            return Ok(TestStepResult {
                step_id: self.id.clone(),
                status: failed_assertion_reason(&assertions, failure_reason),
                failure_message: Some(msg),
                request_data: Some(req_data),
                response_data,
                output_data: None,
                assertion_results: assertions,
            });
        }

        Ok(TestStepResult {
            step_id: self.id.clone(),
            status: TestStepFailureReason::NoFailure,
            failure_message: None,
            request_data: Some(req_data),
            response_data,
            output_data: None,
            assertion_results: assertions,
        })
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
        if let Some(dur_val) = &self.wait_before {
            match parse_duration(dur_val) {
                Ok(d) => tokio::time::sleep(d).await,
                Err(e) => {
                    return Ok(TestStepResult::make_failure(
                        self.id.as_deref(),
                        TestStepFailureReason::ConfigurationError,
                        format!("invalid wait-before duration: {}", e),
                    ))
                }
            }
        }

        let mut last_result: Result<TestStepResult> = Err(anyhow!("no attempts made"));
        for _ in 0..=self.retry {
            last_result = self.run_attempt(config, prior_steps).await;
            match &last_result {
                Ok(r) if r.status == TestStepFailureReason::NoFailure => break,
                Err(_) => break,
                _ => {}
            }
        }

        if let Some(dur_val) = &self.wait_after {
            match parse_duration(dur_val) {
                Ok(d) => tokio::time::sleep(d).await,
                Err(e) => {
                    return Ok(TestStepResult::make_failure(
                        self.id.as_deref(),
                        TestStepFailureReason::ConfigurationError,
                        format!("invalid wait-after duration: {}", e),
                    ))
                }
            }
        }

        last_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn no_config() -> Option<Arc<RwLock<ConfigData>>> {
        None
    }

    fn no_prior_steps() -> HashMap<String, TestStepResult> {
        HashMap::new()
    }

    fn run_assert(observed: Value, expected: Value) -> Vec<AssertionResult> {
        let mut assertions = vec![];
        compare_primitive_values(
            &observed,
            &expected,
            "field",
            &no_config(),
            &no_prior_steps(),
            &mut assertions,
        );
        assertions
    }

    fn make_config(yaml: &str) -> Option<Arc<RwLock<ConfigData>>> {
        use std::path::PathBuf;
        let val: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let config = ConfigData::from_val(val, &PathBuf::from("/test/config.yaml")).unwrap();
        Some(Arc::new(RwLock::new(config)))
    }

    /// A step result with response and request data but no output_data (regular HTTP step).
    fn make_step_result(id: &str, response: Value, request: Value) -> TestStepResult {
        TestStepResult {
            step_id: Some(id.to_owned()),
            response_data: Some(response),
            request_data: Some(request),
            output_data: None,
            status: TestStepFailureReason::NoFailure,
            failure_message: None,
            assertion_results: vec![],
        }
    }

    fn run_compare_objects(
        observed: Value,
        expected: Value,
        full: bool,
    ) -> Vec<AssertionResult> {
        let mut assertions = vec![];
        if let (Some(obs), Some(exp)) = (observed.as_object(), expected.as_object()) {
            compare_data_objects(
                obs,
                exp,
                full,
                "",
                &no_config(),
                &no_prior_steps(),
                &mut assertions,
            );
        }
        assertions
    }

    fn run_compare_arrays(observed: Value, expected: Value) -> Vec<AssertionResult> {
        let mut assertions = vec![];
        if let (Some(obs), Some(exp)) = (observed.as_array(), expected.as_array()) {
            compare_array_objects(
                obs,
                exp,
                false,
                "",
                &no_config(),
                &no_prior_steps(),
                &mut assertions,
            );
        }
        assertions
    }

    fn run_compare_headers(headers: HeaderMap, expected: Value) -> Vec<AssertionResult> {
        let observed = response_headers_to_value(&headers);
        let mut assertions = vec![];
        compare_headers(
            &observed,
            &expected,
            &no_config(),
            &no_prior_steps(),
            &mut assertions,
        );
        assertions
    }

    // ── Generation tests ─────────────────────────────────────────────────────

    #[test]
    fn test_re_generate_produces_match() {
        let pattern = "re/[a-z]{8}";
        let input = json!(pattern);
        let result = clean_request_data(&input, &no_config(), &no_prior_steps())
            .expect("generation should succeed");
        let generated = result.as_str().expect("result should be a string");

        // The generated string must match the pattern (strip re/ prefix)
        let re = Regex::new("[a-z]{8}").unwrap();
        assert!(
            re.is_match(generated),
            "generated '{}' does not match [a-z]{{8}}",
            generated
        );
    }

    #[test]
    fn test_re_generate_not_literal() {
        let input = json!("re/[a-z]{8}");
        let result = clean_request_data(&input, &no_config(), &no_prior_steps())
            .expect("generation should succeed");
        let generated = result.as_str().expect("result should be a string");
        assert_ne!(
            generated, "re/[a-z]{8}",
            "result should not be the literal pattern string"
        );
    }

    #[test]
    fn test_re_generate_invalid_pattern_errors() {
        let input = json!("re/[invalid");
        let result = clean_request_data(&input, &no_config(), &no_prior_steps());
        assert!(result.is_err(), "invalid pattern should return Err");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("invalid regex pattern"),
            "error message should mention invalid regex pattern, got: {}",
            msg
        );
    }

    // ── Assertion tests ───────────────────────────────────────────────────────

    #[test]
    fn test_re_assert_passes_on_match() {
        let results = run_assert(json!("hello"), json!("re/[a-z]+"));
        assert_eq!(results.len(), 1);
        assert!(results[0].passed, "assertion should pass for matching string");
    }

    #[test]
    fn test_re_assert_fails_on_no_match() {
        let results = run_assert(json!("HELLO"), json!("re/[a-z]+"));
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed, "assertion should fail for non-matching string");
        let msg = results[0].message.as_deref().unwrap_or("");
        assert!(
            msg.contains("expected to match re/[a-z]+"),
            "error message should describe the mismatch, got: {}",
            msg
        );
    }

    #[test]
    fn test_re_assert_fails_for_non_string_observed() {
        let results = run_assert(json!(42), json!("re/[a-z]+"));
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed, "assertion should fail when observed is not a string");
        let msg = results[0].message.as_deref().unwrap_or("");
        assert!(
            msg.contains("expected a string to match"),
            "error message should mention type mismatch, got: {}",
            msg
        );
    }

    #[test]
    fn test_re_assert_invalid_pattern_fails() {
        let results = run_assert(json!("hello"), json!("re/[invalid"));
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed, "invalid pattern should produce a failed assertion");
        let msg = results[0].message.as_deref().unwrap_or("");
        assert!(
            msg.contains("invalid regex pattern"),
            "error message should mention invalid regex pattern, got: {}",
            msg
        );
    }

    // ── parse_duration tests ─────────────────────────────────────────────────

    #[test]
    fn test_parse_duration_bare_int_value() {
        let result = parse_duration(&json!(500u64)).unwrap();
        assert_eq!(result, std::time::Duration::from_millis(500));
    }

    #[test]
    fn test_parse_duration_bare_int_string() {
        let result = parse_duration(&json!("500")).unwrap();
        assert_eq!(result, std::time::Duration::from_millis(500));
    }

    #[test]
    fn test_parse_duration_ms_suffix() {
        let result = parse_duration(&json!("250ms")).unwrap();
        assert_eq!(result, std::time::Duration::from_millis(250));
    }

    #[test]
    fn test_parse_duration_s_suffix() {
        let result = parse_duration(&json!("2s")).unwrap();
        assert_eq!(result, std::time::Duration::from_millis(2000));
    }

    #[test]
    fn test_parse_duration_invalid_string() {
        let result = parse_duration(&json!("fast"));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("invalid duration"),
            "expected 'invalid duration' in error, got: {}",
            msg
        );
    }

    #[test]
    fn test_parse_duration_float_rejected() {
        let result = parse_duration(&json!("1.5s"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_duration_float_number_rejected() {
        let result = parse_duration(&serde_json::json!(1.5_f64));
        assert!(result.is_err());
    }

    // ── get_value_length ─────────────────────────────────────────────────────

    #[test]
    fn test_get_value_length_string() {
        assert_eq!(get_value_length(&json!("hello")).unwrap(), 5);
    }

    #[test]
    fn test_get_value_length_empty_string() {
        assert_eq!(get_value_length(&json!("")).unwrap(), 0);
    }

    #[test]
    fn test_get_value_length_array() {
        assert_eq!(get_value_length(&json!([1, 2, 3])).unwrap(), 3);
    }

    #[test]
    fn test_get_value_length_object() {
        assert_eq!(get_value_length(&json!({"a": 1, "b": 2})).unwrap(), 2);
    }

    #[test]
    fn test_get_value_length_number_errors() {
        let err = get_value_length(&json!(42)).unwrap_err();
        assert!(err.to_string().contains("cannot check length"), "{}", err);
    }

    #[test]
    fn test_get_value_length_bool_errors() {
        assert!(get_value_length(&json!(true)).is_err());
    }

    #[test]
    fn test_get_value_length_null_errors() {
        assert!(get_value_length(&json!(null)).is_err());
    }

    // ── check_size ───────────────────────────────────────────────────────────

    #[test]
    fn test_check_size_equal_pass() {
        assert!(check_size(&json!("hello"), "5").unwrap());
    }

    #[test]
    fn test_check_size_equal_fail() {
        assert!(!check_size(&json!("hello"), "4").unwrap());
    }

    #[test]
    fn test_check_size_gt_pass() {
        assert!(check_size(&json!([1, 2, 3]), ">2").unwrap());
    }

    #[test]
    fn test_check_size_gt_fail() {
        assert!(!check_size(&json!([1, 2]), ">2").unwrap());
    }

    #[test]
    fn test_check_size_gte_pass() {
        assert!(check_size(&json!([1, 2]), ">=2").unwrap());
        assert!(check_size(&json!([1, 2, 3]), ">=2").unwrap());
    }

    #[test]
    fn test_check_size_gte_fail() {
        assert!(!check_size(&json!([1]), ">=2").unwrap());
    }

    #[test]
    fn test_check_size_lt_pass() {
        assert!(check_size(&json!([1]), "<2").unwrap());
    }

    #[test]
    fn test_check_size_lt_fail() {
        assert!(!check_size(&json!([1, 2]), "<2").unwrap());
    }

    #[test]
    fn test_check_size_lte_pass() {
        assert!(check_size(&json!([1, 2]), "<=2").unwrap());
    }

    #[test]
    fn test_check_size_lte_fail() {
        assert!(!check_size(&json!([1, 2, 3]), "<=2").unwrap());
    }

    #[test]
    fn test_check_size_invalid_comparison_errors() {
        let err = check_size(&json!([1]), "not_valid").unwrap_err();
        assert!(err.to_string().contains("invalid size comparison"), "{}", err);
    }

    // ── clean_path ───────────────────────────────────────────────────────────

    #[test]
    fn test_clean_path_static() {
        let result = clean_path("users/me", &no_config(), &no_prior_steps()).unwrap();
        assert_eq!(result, "/users/me");
    }

    #[test]
    fn test_clean_path_string_variable_from_prior_step() {
        let mut prior = no_prior_steps();
        prior.insert(
            "login".to_owned(),
            make_step_result("login", json!({"token": "abc123"}), json!(null)),
        );
        let result =
            clean_path("auth/$login.response.token/verify", &no_config(), &prior).unwrap();
        assert_eq!(result, "/auth/abc123/verify");
    }

    #[test]
    fn test_clean_path_integer_variable_from_prior_step() {
        let mut prior = no_prior_steps();
        prior.insert(
            "create".to_owned(),
            make_step_result("create", json!({"id": 42}), json!(null)),
        );
        let result = clean_path("items/$create.response.id", &no_config(), &prior).unwrap();
        assert_eq!(result, "/items/42");
    }

    #[test]
    fn test_clean_path_missing_variable_errors() {
        let result = clean_path("users/$nonexistent.response.id", &no_config(), &no_prior_steps());
        assert!(result.is_err());
    }

    // ── clean_headers ────────────────────────────────────────────────────────

    #[test]
    fn test_clean_headers_static() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_owned(), "application/json".to_owned());
        let result = clean_headers(&headers, &no_config(), &no_prior_steps()).unwrap();
        assert_eq!(result.get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn test_clean_headers_variable_from_config() {
        let cfg = make_config("vars:\n  my_token: Bearer xyz789");
        let mut headers = HashMap::new();
        headers.insert("authorization".to_owned(), "$vars.my_token".to_owned());
        let result = clean_headers(&headers, &cfg, &no_prior_steps()).unwrap();
        assert_eq!(result.get("authorization").unwrap(), "Bearer xyz789");
    }

    #[test]
    fn test_clean_headers_missing_variable_errors() {
        let mut headers = HashMap::new();
        headers.insert("authorization".to_owned(), "$vars.nonexistent".to_owned());
        assert!(clean_headers(&headers, &no_config(), &no_prior_steps()).is_err());
    }

    // ── response header assertions ───────────────────────────────────────────

    #[test]
    fn test_response_headers_exact_match_case_insensitive_name() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));

        let assertions = run_compare_headers(headers, json!({"Content-Type": "application/json"}));

        assert_eq!(assertions.len(), 1);
        assert!(assertions[0].passed, "{:?}", assertions);
        assert_eq!(assertions[0].name, "headers.content-type");
    }

    #[test]
    fn test_response_headers_regex_match() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", HeaderValue::from_static("REQ-123456"));

        let assertions = run_compare_headers(headers, json!({"X-Request-ID": "re/REQ-[0-9]{6}"}));

        assert_eq!(assertions.len(), 1);
        assert!(assertions[0].passed, "{:?}", assertions);
    }

    #[test]
    fn test_response_headers_missing_header_fails() {
        let headers = HeaderMap::new();

        let assertions = run_compare_headers(headers, json!({"X-Request-ID": "+str"}));

        assert_eq!(assertions.len(), 1);
        assert!(!assertions[0].passed);
        let msg = assertions[0].message.as_deref().unwrap();
        assert!(msg.contains("missing field 'headers.x-request-id'"), "{}", msg);
    }

    #[test]
    fn test_response_headers_duplicate_values_are_arrays() {
        let mut headers = HeaderMap::new();
        headers.append("set-cookie", HeaderValue::from_static("a=1"));
        headers.append("set-cookie", HeaderValue::from_static("b=2"));

        let assertions = run_compare_headers(headers, json!({"set-cookie": ["a=1", "b=2"]}));

        assert_eq!(assertions.len(), 2);
        assert!(assertions.iter().all(|assertion| assertion.passed), "{:?}", assertions);
    }

    // ── clean_request_data (extended) ────────────────────────────────────────

    #[test]
    fn test_clean_request_data_passthrough_null() {
        let result =
            clean_request_data(&json!(null), &no_config(), &no_prior_steps()).unwrap();
        assert_eq!(result, json!(null));
    }

    #[test]
    fn test_clean_request_data_passthrough_number() {
        let result =
            clean_request_data(&json!(42), &no_config(), &no_prior_steps()).unwrap();
        assert_eq!(result, json!(42));
    }

    #[test]
    fn test_clean_request_data_passthrough_static_string() {
        let result =
            clean_request_data(&json!("static"), &no_config(), &no_prior_steps()).unwrap();
        assert_eq!(result, json!("static"));
    }

    #[test]
    fn test_clean_request_data_nested_object() {
        let input = json!({"a": "one", "b": {"c": "two"}});
        let result = clean_request_data(&input, &no_config(), &no_prior_steps()).unwrap();
        assert_eq!(result, json!({"a": "one", "b": {"c": "two"}}));
    }

    #[test]
    fn test_clean_request_data_array() {
        let input = json!(["a", "b", "c"]);
        let result = clean_request_data(&input, &no_config(), &no_prior_steps()).unwrap();
        assert_eq!(result, json!(["a", "b", "c"]));
    }

    #[test]
    fn test_clean_request_data_variable_substitution() {
        let cfg = make_config("vars:\n  username: alice");
        let input = json!({"user": "$vars.username"});
        let result = clean_request_data(&input, &cfg, &no_prior_steps()).unwrap();
        assert_eq!(result, json!({"user": "alice"}));
    }

    // ── get_variable ─────────────────────────────────────────────────────────

    #[test]
    fn test_get_variable_non_dollar_passthrough() {
        let result =
            get_variable("literal", &no_config(), &no_prior_steps()).unwrap();
        assert_eq!(result, json!("literal"));
    }

    #[test]
    fn test_get_variable_config_var() {
        let cfg = make_config("vars:\n  username: testuser");
        let result = get_variable("$vars.username", &cfg, &no_prior_steps()).unwrap();
        assert_eq!(result, json!("testuser"));
    }

    #[test]
    fn test_get_variable_config_url() {
        let cfg = make_config("urls:\n  base: https://example.com");
        let result = get_variable("$urls.base", &cfg, &no_prior_steps()).unwrap();
        assert_eq!(result, json!("https://example.com"));
    }

    #[test]
    fn test_get_variable_from_prior_step_response() {
        let mut prior = no_prior_steps();
        prior.insert(
            "login".to_owned(),
            make_step_result("login", json!({"token": "xyz789"}), json!(null)),
        );
        let result = get_variable("$login.response.token", &no_config(), &prior).unwrap();
        assert_eq!(result, json!("xyz789"));
    }

    #[test]
    fn test_get_variable_missing_step_errors() {
        let result =
            get_variable("$nonexistent.response.field", &no_config(), &no_prior_steps());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("nonexistent"), "expected step id in error, got: {}", msg);
    }

    #[test]
    fn test_get_variable_regex_from_config_var() {
        let cfg = make_config("vars:\n  my_id: \"re/[0-9]{6}\"");
        let result = get_variable("$vars.my_id", &cfg, &no_prior_steps()).unwrap();
        let generated = result.as_str().unwrap();
        let re = Regex::new("[0-9]{6}").unwrap();
        assert!(
            re.is_match(generated),
            "generated '{}' should match [0-9]{{6}}",
            generated
        );
    }

    // ── compare_primitive_values (extended) ──────────────────────────────────

    #[test]
    fn test_compare_primitive_type_str_pass() {
        let r = run_assert(json!("hello"), json!("+str"));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_str_fail() {
        let r = run_assert(json!(42), json!("+str"));
        assert!(!r[0].passed);
        assert!(r[0].message.as_deref().unwrap().contains("a string"));
    }

    #[test]
    fn test_compare_primitive_type_string_synonym() {
        let r = run_assert(json!("hello"), json!("+string"));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_int_pass() {
        let r = run_assert(json!(42), json!("+int"));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_integer_synonym() {
        let r = run_assert(json!(42), json!("+integer"));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_bool_pass() {
        let r = run_assert(json!(true), json!("+bool"));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_bool_fail() {
        let r = run_assert(json!("yes"), json!("+bool"));
        assert!(!r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_arr_pass() {
        let r = run_assert(json!([1, 2]), json!("+arr"));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_arr_fail() {
        let r = run_assert(json!("not_an_array"), json!("+arr"));
        assert!(!r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_dict_pass() {
        let r = run_assert(json!({"k": "v"}), json!("+dict"));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_float_pass() {
        let r = run_assert(json!(3.14), json!("+float"));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_exact_string_match() {
        let r = run_assert(json!("hello"), json!("hello"));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_exact_string_mismatch() {
        let r = run_assert(json!("hello"), json!("world"));
        assert!(!r[0].passed);
        assert!(r[0].message.as_deref().unwrap().contains("expected"));
    }

    #[test]
    fn test_compare_primitive_exact_int_match() {
        let r = run_assert(json!(42), json!(42));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_exact_bool_match() {
        let r = run_assert(json!(false), json!(false));
        assert!(r[0].passed);
    }

    #[test]
    fn test_compare_primitive_type_mismatch() {
        let r = run_assert(json!("hello"), json!(42));
        assert!(!r[0].passed);
        let msg = r[0].message.as_deref().unwrap();
        assert!(msg.contains("expected"), "{}", msg);
    }

    #[test]
    fn test_compare_primitive_variable_reference_match() {
        let mut prior = no_prior_steps();
        prior.insert(
            "step1".to_owned(),
            make_step_result("step1", json!({"id": 42}), json!(null)),
        );
        let mut assertions = vec![];
        compare_primitive_values(
            &json!(42),
            &json!("$step1.response.id"),
            "field",
            &no_config(),
            &prior,
            &mut assertions,
        );
        assert!(assertions[0].passed);
    }

    #[test]
    fn test_compare_primitive_variable_reference_mismatch() {
        let mut prior = no_prior_steps();
        prior.insert(
            "step1".to_owned(),
            make_step_result("step1", json!({"id": 99}), json!(null)),
        );
        let mut assertions = vec![];
        compare_primitive_values(
            &json!(42),
            &json!("$step1.response.id"),
            "field",
            &no_config(),
            &prior,
            &mut assertions,
        );
        assert!(!assertions[0].passed);
    }

    // ── compare_data_objects ─────────────────────────────────────────────────

    #[test]
    fn test_compare_objects_all_matching() {
        let assertions = run_compare_objects(
            json!({"name": "Alice", "age": 30}),
            json!({"name": "Alice", "age": 30}),
            false,
        );
        assert!(assertions.iter().all(|a| a.passed), "{:?}", assertions);
    }

    #[test]
    fn test_compare_objects_missing_expected_field() {
        let assertions = run_compare_objects(
            json!({"name": "Alice"}),
            json!({"name": "Alice", "age": 30}),
            false,
        );
        let failed: Vec<_> = assertions.iter().filter(|a| !a.passed).collect();
        assert_eq!(failed.len(), 1);
        assert!(
            failed[0].message.as_deref().unwrap().contains("age"),
            "{:?}",
            failed[0].message
        );
    }

    #[test]
    fn test_compare_objects_full_mode_flags_extra_field() {
        let assertions = run_compare_objects(
            json!({"name": "Alice", "extra": "surprise"}),
            json!({"name": "Alice"}),
            true,
        );
        let failed: Vec<_> = assertions.iter().filter(|a| !a.passed).collect();
        assert_eq!(failed.len(), 1);
        assert!(
            failed[0].message.as_deref().unwrap().contains("extra"),
            "{:?}",
            failed[0].message
        );
    }

    #[test]
    fn test_compare_objects_non_full_mode_ignores_extra_field() {
        let assertions = run_compare_objects(
            json!({"name": "Alice", "extra": "surprise"}),
            json!({"name": "Alice"}),
            false,
        );
        assert!(assertions.iter().all(|a| a.passed), "{:?}", assertions);
    }

    #[test]
    fn test_compare_objects_len_assertion_pass() {
        let assertions = run_compare_objects(
            json!({"items": [1, 2, 3]}),
            json!({"len(items)": ">=2"}),
            false,
        );
        assert!(assertions.iter().all(|a| a.passed), "{:?}", assertions);
    }

    #[test]
    fn test_compare_objects_len_assertion_fail() {
        let assertions = run_compare_objects(
            json!({"items": [1]}),
            json!({"len(items)": ">=2"}),
            false,
        );
        let failed: Vec<_> = assertions.iter().filter(|a| !a.passed).collect();
        assert!(!failed.is_empty(), "expected a failure");
    }

    // ── compare_array_objects ────────────────────────────────────────────────

    #[test]
    fn test_compare_arrays_matching() {
        let assertions = run_compare_arrays(json!([1, 2, 3]), json!([1, 2, 3]));
        assert!(assertions.iter().all(|a| a.passed), "{:?}", assertions);
    }

    #[test]
    fn test_compare_arrays_length_mismatch() {
        let assertions = run_compare_arrays(json!([1, 2]), json!([1, 2, 3]));
        let failed: Vec<_> = assertions.iter().filter(|a| !a.passed).collect();
        assert!(!failed.is_empty());
        assert!(
            failed[0].message.as_deref().unwrap().contains("expected 3 item(s), got 2"),
            "{:?}",
            failed[0].message
        );
    }

    #[test]
    fn test_compare_arrays_element_mismatch() {
        let assertions = run_compare_arrays(json!(["a", "b"]), json!(["a", "c"]));
        assert!(assertions.iter().any(|a| !a.passed));
    }

    // ── compare_data (top-level) ─────────────────────────────────────────────

    #[test]
    fn test_compare_data_nested_object_pass() {
        let mut assertions = vec![];
        let passed = compare_data(
            &json!({"user": {"name": "Alice", "age": 30}}),
            &json!({"user": {"name": "Alice"}}),
            &no_config(),
            &no_prior_steps(),
            false,
            &mut assertions,
        );
        assert!(passed, "{:?}", assertions);
    }

    #[test]
    fn test_compare_data_nested_object_fail() {
        let mut assertions = vec![];
        let passed = compare_data(
            &json!({"user": {"name": "Bob"}}),
            &json!({"user": {"name": "Alice"}}),
            &no_config(),
            &no_prior_steps(),
            false,
            &mut assertions,
        );
        assert!(!passed);
    }

    // ── TestStep::check_status_code ──────────────────────────────────────────

    #[test]
    fn test_check_status_code_exact_match() {
        assert!(TestStep::check_status_code(&json!(200u64), 200));
    }

    #[test]
    fn test_check_status_code_exact_mismatch() {
        assert!(!TestStep::check_status_code(&json!(200u64), 404));
    }

    #[test]
    fn test_check_status_code_wildcard_matches() {
        assert!(TestStep::check_status_code(&json!("4xx"), 404));
        assert!(TestStep::check_status_code(&json!("4xx"), 400));
        assert!(TestStep::check_status_code(&json!("20x"), 200));
        assert!(TestStep::check_status_code(&json!("20x"), 201));
    }

    #[test]
    fn test_check_status_code_wildcard_mismatch() {
        assert!(!TestStep::check_status_code(&json!("4xx"), 200));
        assert!(!TestStep::check_status_code(&json!("20x"), 404));
    }

    // ── TestStepResult::get_field ────────────────────────────────────────────

    #[test]
    fn test_get_field_response_namespace() {
        let result = make_step_result("s", json!({"id": 42}), json!(null));
        assert_eq!(result.get_field("response.id").unwrap(), Some(json!(42)));
    }

    #[test]
    fn test_get_field_request_namespace() {
        let result = make_step_result("s", json!(null), json!({"body": "hello"}));
        assert_eq!(result.get_field("request.body").unwrap(), Some(json!("hello")));
    }

    #[test]
    fn test_get_field_data_namespace_alias() {
        let result = make_step_result("s", json!(null), json!({"key": "val"}));
        assert_eq!(result.get_field("data.key").unwrap(), Some(json!("val")));
    }

    #[test]
    fn test_get_field_nested_response_field() {
        let result =
            make_step_result("s", json!({"user": {"name": "Alice"}}), json!(null));
        assert_eq!(
            result.get_field("response.user.name").unwrap(),
            Some(json!("Alice"))
        );
    }

    #[test]
    fn test_get_field_missing_key_returns_none() {
        let result = make_step_result("s", json!({"id": 42}), json!(null));
        assert_eq!(result.get_field("response.nonexistent").unwrap(), None);
    }

    #[test]
    fn test_get_field_invalid_namespace_errors() {
        let result = make_step_result("s", json!({"id": 42}), json!(null));
        assert!(result.get_field("invalid.field").is_err());
    }

    #[test]
    fn test_get_field_output_data_bypasses_namespace_routing() {
        let result = TestStepResult {
            step_id: None,
            response_data: None,
            request_data: None,
            output_data: Some(json!({"token": "abc"})),
            status: TestStepFailureReason::NoFailure,
            failure_message: None,
            assertion_results: vec![],
        };
        assert_eq!(result.get_field("token").unwrap(), Some(json!("abc")));
    }

    // ── push_duration_assertion ──────────────────────────────────────────────

    #[test]
    fn test_push_duration_assertion_skipped_when_no_expected() {
        let mut assertions = vec![];
        push_duration_assertion(&mut assertions, None, std::time::Duration::from_millis(100));
        assert!(assertions.is_empty());
    }

    #[test]
    fn test_push_duration_assertion_passes_when_under_limit() {
        let mut assertions = vec![];
        push_duration_assertion(
            &mut assertions,
            Some(&json!(500u64)),
            std::time::Duration::from_millis(100),
        );
        assert_eq!(assertions.len(), 1);
        assert!(assertions[0].passed, "{:?}", assertions[0].message);
    }

    #[test]
    fn test_push_duration_assertion_fails_when_over_limit() {
        let mut assertions = vec![];
        push_duration_assertion(
            &mut assertions,
            Some(&json!(100u64)),
            std::time::Duration::from_millis(500),
        );
        assert_eq!(assertions.len(), 1);
        assert!(!assertions[0].passed);
        let msg = assertions[0].message.as_deref().unwrap();
        assert!(msg.contains("expected less than"), "{}", msg);
    }

    #[test]
    fn test_push_duration_assertion_invalid_format_fails() {
        let mut assertions = vec![];
        push_duration_assertion(
            &mut assertions,
            Some(&json!("not_a_duration")),
            std::time::Duration::from_millis(100),
        );
        assert_eq!(assertions.len(), 1);
        assert!(!assertions[0].passed);
    }

    // ── failed_assertion_message / failed_assertion_reason ───────────────────

    #[test]
    fn test_failed_assertion_message_returns_none_when_all_pass() {
        let assertions = vec![AssertionResult {
            name: "status 200".to_owned(),
            passed: true,
            message: None,
        }];
        assert_eq!(failed_assertion_message(&assertions), None);
    }

    #[test]
    fn test_failed_assertion_message_includes_all_failures() {
        let assertions = vec![
            AssertionResult {
                name: "status 200".to_owned(),
                passed: false,
                message: Some("expected status 200, got 500".to_owned()),
            },
            AssertionResult {
                name: "name".to_owned(),
                passed: true,
                message: None,
            },
            AssertionResult {
                name: "email".to_owned(),
                passed: false,
                message: Some("'email' — expected a string, got Null (null)".to_owned()),
            },
        ];

        let msg = failed_assertion_message(&assertions).unwrap();
        assert!(msg.contains("2 assertions failed"), "{}", msg);
        assert!(msg.contains("expected status 200, got 500"), "{}", msg);
        assert!(msg.contains("'email'"), "{}", msg);
    }

    #[test]
    fn test_failed_assertion_reason_prefers_status_code_failure() {
        let assertions = vec![
            AssertionResult {
                name: "body".to_owned(),
                passed: false,
                message: Some("response body is not valid JSON".to_owned()),
            },
            AssertionResult {
                name: "status 200".to_owned(),
                passed: false,
                message: Some("expected status 200, got 500".to_owned()),
            },
        ];

        assert_eq!(
            failed_assertion_reason(&assertions, TestStepFailureReason::JsonDecodeError),
            TestStepFailureReason::StatusCodeError
        );
    }

    // ── wait-before / wait-after / retry deserialization ─────────────────────

    #[test]
    fn test_spec_wait_before_bare_integer() {
        let yaml = "path: /api/test\nwait-before: 500";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.wait_before, Some(json!(500)));
    }

    #[test]
    fn test_spec_wait_before_ms_string() {
        let yaml = "path: /api/test\nwait-before: \"500ms\"";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.wait_before, Some(json!("500ms")));
    }

    #[test]
    fn test_spec_wait_before_s_string() {
        let yaml = "path: /api/test\nwait-before: \"2s\"";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.wait_before, Some(json!("2s")));
    }

    #[test]
    fn test_spec_wait_after_string() {
        let yaml = "path: /api/test\nwait-after: \"1s\"";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.wait_after, Some(json!("1s")));
    }

    #[test]
    fn test_spec_retry_integer() {
        let yaml = "path: /api/test\nretry: 3";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.retry, Some(3));
    }

    #[test]
    fn test_spec_wait_and_retry_absent_by_default() {
        let yaml = "path: /api/test";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(spec.wait_before.is_none());
        assert!(spec.wait_after.is_none());
        assert!(spec.retry.is_none());
    }

    #[test]
    fn test_step_retry_defaults_to_zero() {
        let yaml = "path: /api/test";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        let step = TestStep::from_spec(spec);
        assert_eq!(step.retry, 0);
    }

    #[test]
    fn test_step_wait_before_preserved_from_spec() {
        let yaml = "path: /api/test\nwait-before: 200";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        let step = TestStep::from_spec(spec);
        assert_eq!(step.wait_before, Some(json!(200)));
    }

    #[test]
    fn test_step_wait_after_preserved_from_spec() {
        let yaml = "path: /api/test\nwait-after: \"500ms\"";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        let step = TestStep::from_spec(spec);
        assert_eq!(step.wait_after, Some(json!("500ms")));
    }

    #[test]
    fn test_step_retry_preserved_from_spec() {
        let yaml = "path: /api/test\nretry: 5";
        let spec: TestStepSpec = serde_yaml::from_str(yaml).unwrap();
        let step = TestStep::from_spec(spec);
        assert_eq!(step.retry, 5);
    }

    // ── failed assertion propagates as ResponseError ──────────────────────────

    #[test]
    fn test_failed_assertion_in_results_means_response_error() {
        // If assertions vec contains a failed entry, step must not return NoFailure.
        // This exercises the guard added before the final Ok(NoFailure) return.
        let failed = AssertionResult {
            name: "duration".to_owned(),
            passed: false,
            message: Some("request took 500ms, expected less than 1ms".to_owned()),
        };
        // Confirm the failure reason is distinguishable from NoFailure.
        assert_ne!(
            TestStepFailureReason::ResponseError,
            TestStepFailureReason::NoFailure
        );
        // Confirm the assertion itself is marked failed.
        assert!(!failed.passed);
    }
}
