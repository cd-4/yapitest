use crate::config::ConfigData;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde::Deserialize;

use serde_json::{Map, Value};
use std::any::Any;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fs::write;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use std::mem::discriminant;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TestStepFailureReason {
    NoFailure,
    NoResponse,
    ResponseError,
    StatusCodeError,
    JsonDecodeError,
    ConfigurationError,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TestStepStatus {
    NotRun,
    InProgress,
    Pass,
    Fail,
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
    output: Option<HashMap<String, String>>,
}

impl Display for TestStepStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            TestStepStatus::Pass => write!(f, "Pass"),
            TestStepStatus::InProgress => write!(f, "In Progress"),
            TestStepStatus::Fail => write!(f, "Fail"),
            TestStepStatus::NotRun => write!(f, "Not Run"),
        }
    }
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
    status: TestStepStatus,
    failure_reason: TestStepFailureReason,
}

pub struct TestStepResult {
    pub response_data: Option<Value>,
    pub request_data: Option<Value>,
    pub output_data: Option<Value>,
    pub status: TestStepFailureReason,
    pub failure_message: Option<String>,
}

pub fn get_variable(
    name: String,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<Value> {
    if !name.starts_with('$') {
        return Ok(Value::String(name));
    }
    let mut current_key = name.clone();
    'outer: while current_key.starts_with('$') {
        let mut value_key = current_key.clone();
        value_key.remove(0);

        if let Some(cfg) = config {
            if let Ok(new_val) = cfg.read().unwrap().get_string_value(value_key.clone()) {
                current_key = new_val;
                if current_key.starts_with('$') {
                    continue 'outer;
                } else {
                    return Ok(Value::from(current_key));
                }
            }
        }

        let mut segments: Vec<String> = value_key
            .clone()
            .split('.')
            .map(|v| v.to_string())
            .collect();

        println!("KEY: {}", current_key);

        if let Some(step) = segments.first().and_then(|v| prior_steps.get(v)) {
            segments.remove(0);
            let field_key = segments.join(".");
            println!("Found Step {}", field_key);
            match step.get_field(field_key.clone()) {
                Ok(field_val) => {
                    if let Some(val) = field_val {
                        if let Some(value_str) = val.as_str() {
                            if value_str.starts_with("$") {
                                current_key = value_str.to_string();
                                continue 'outer;
                            }
                        }
                        return Ok(val);
                    } else {
                        return Err(anyhow!("Value {} not found", field_key));
                    }
                }
                Err(e) => {
                    return Err(anyhow!("Value {} not found", field_key));
                }
            }
        }

        return Err(anyhow!("2 Value not found: {}", name));
    }
    Err(anyhow!("3 Value not found: {}", name))
}

pub fn clean_request_data(
    request_data: &Value,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<Value> {
    if let Some(data_map) = request_data.as_object() {
        let mut new_value = data_map.clone();
        for (k, v) in data_map.iter() {
            match clean_request_data(v, config, prior_steps) {
                Ok(val) => {
                    new_value.insert(k.clone(), val);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(Value::from(new_value))
    } else if let Some(data_arr) = request_data.as_array() {
        let mut new_val: Vec<Value> = data_arr.clone();
        for item in data_arr.iter() {
            match clean_request_data(item, config, prior_steps) {
                Ok(val) => {
                    new_val.push(val);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(Value::from(new_val))
    } else if let Some(data_str) = request_data.as_str() {
        if data_str.starts_with("$") {
            match get_variable(data_str.to_string(), config, prior_steps) {
                Ok(var) => Ok(var),
                Err(e) => Err(e),
            }
        } else {
            Ok(Value::from(data_str))
        }
    } else {
        Ok(request_data.clone())
    }
}

pub fn clean_path(
    path: String,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<String> {
    let ends_with_slash = path.ends_with("/");

    let mut segments: Vec<String> = vec![];

    for segment in path.split("/") {
        if segment.starts_with("$") {
            let new_seg = get_variable(segment.to_string(), config, prior_steps)?;
            segments.push(new_seg.to_string());
        } else {
            segments.push(segment.to_string());
        }
    }
    let mut output = segments.join("/");
    output = format!("/{}", output);
    if ends_with_slash {
        output = format!("{}/", output);
    }
    Ok(output)
}

pub fn clean_headers(
    header_data: &HashMap<String, String>,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<HeaderMap> {
    let mut output: HeaderMap = HeaderMap::new();
    for (k, v) in header_data.iter() {
        if v.starts_with("$") {
            match get_variable(v.to_string(), config, prior_steps) {
                Ok(header_value) => {
                    if let Some(header_str) = header_value.as_str() {
                        let name = HeaderName::from_bytes(k.as_bytes()).unwrap();
                        let val = HeaderValue::from_str(header_str).unwrap();
                        output.insert(name, val);
                    } else {
                        return Err(anyhow!("Invalid Header {}: {}", k, v));
                    }
                }
                Err(e) => {
                    return Err(anyhow!("Invalid Header {}: {} ({})", k, v, e));
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

pub fn clean_data(
    value: &Value,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<Value> {
    if let Some(map) = value.clone().as_object_mut() {
        for (key, val) in map.clone().iter_mut() {
            match clean_data(val, config, prior_steps) {
                Ok(cleaned) => {
                    map.insert(key.clone(), cleaned);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        return Ok(Value::from(map.clone()));
    } else if let Some(vec) = value.clone().as_array_mut() {
        let mut cleaned_vec: Vec<Value> = Vec::with_capacity(vec.len());

        for item in vec.iter_mut() {
            match clean_data(item, config, prior_steps) {
                Ok(cleaned) => {
                    cleaned_vec.push(cleaned);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        return Ok(Value::Array(cleaned_vec));
    } else if let Some(str) = value.as_str() {
        return get_variable(str.to_string(), config, prior_steps);
    }

    return Ok(value.clone());
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
    value: i64, // or f64 if you need floats
}

fn parse_comparison(s: &str) -> Result<Comparison, String> {
    // Matches optional spaces around the operator and number
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
        "=" | "" => Operator::Eq, // allow "=" or even just the number (treat as =)
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

    Err(anyhow!("Size could not be determined for {}", val))
}

pub fn check_size(val: &Value, size_str: String) -> Result<bool> {
    let value_size = get_value_length(val)?;

    match parse_comparison(&size_str) {
        Ok(cmp) => match cmp.op {
            Operator::Gt => {
                return Ok(value_size > cmp.value);
            }
            Operator::Lt => {
                return Ok(value_size < cmp.value);
            }
            Operator::Eq => {
                return Ok(value_size == cmp.value);
            }
            Operator::Gte => {
                return Ok(value_size >= cmp.value);
            }
            Operator::Lte => {
                return Ok(value_size <= cmp.value);
            }
        },
        Err(e) => {
            return Err(anyhow!("Unable to pare comparison: {}", e));
        }
    }
}

pub fn compare_data_objects(
    observed_object: &Map<String, Value>,
    expected_object: &Map<String, Value>,
    full: bool,
    keys: String,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<()> {
    for key in observed_object.keys() {
        let observed = observed_object.get(key).unwrap();
        let exp_value = expected_object.get(key);

        if exp_value.is_none() {
            let size_str: String = format!("len({})", key);
            if let Some(expected_size) = expected_object.get(&size_str) {
                match check_size(observed, expected_size.as_str().unwrap().to_string()) {
                    Ok(is_correct_size) => {
                        if !is_correct_size {
                            return Err(anyhow!(
                                "Incorrect Size: len({}.{}) !{}",
                                keys,
                                key,
                                expected_size
                            ));
                        }
                    }
                    Err(e) => {}
                }
            } else if full {
                return Err(anyhow!(
                    "'full' is set and value '{}.{}' was not found",
                    keys,
                    key
                ));
            }
            continue;
        }

        let expected = exp_value.unwrap();

        compare_data_inner(
            observed,
            expected,
            full,
            format!("{}.{}", keys, key),
            config,
            prior_steps,
        )?;
    }

    Ok(())
}

pub fn compare_array_objects(
    observed_object: &Vec<Value>,
    expected_object: &Vec<Value>,
    full: bool,
    keys: String,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<()> {
    let num_expected = expected_object.len();
    let num_observed = observed_object.len();
    if num_expected != num_observed {
        return Err(anyhow!(
            "Expected {} items in {}. Found {}",
            num_expected,
            keys,
            num_observed
        ));
    }

    for (index, (observed, expected)) in observed_object
        .iter()
        .zip(expected_object.iter())
        .enumerate()
    {
        let new_keys = format!("{}.[{}]", keys, index);
        compare_data_inner(observed, expected, full, new_keys, config, prior_steps)?;
    }

    Ok(())
}

pub fn compare_primitive_values(
    observed: &Value,
    expected: &Value,
    keys: String,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<()> {
    if let Some(exp_str) = expected.as_str() {
        if exp_str.starts_with('+') {
            let mut exp_type = exp_str.to_string();
            exp_type.remove(0);
            if (exp_type == "str" || exp_type == "string") {
                if observed.as_str().is_none() {
                    return Err(anyhow!("Expected string for {}", keys));
                } else {
                    return Ok(());
                }
            } else if (exp_type == "float" || exp_type == "flt") {
                if observed.as_f64().is_none() {
                    return Err(anyhow!("Expected float for {}", keys));
                } else {
                    return Ok(());
                }
            } else if exp_type == "int" && observed.as_i64().is_none() {
                if observed.as_i64().is_none() {
                    return Err(anyhow!("Expected int for {}", keys));
                } else {
                    return Ok(());
                }
            }
        } else if exp_str.starts_with("$") {
            println!("==== GEtTING EXPECTED ====");
            let exp_var = get_variable(exp_str.to_string(), config, prior_steps)?;
            println!("Expected {}", exp_var);
            if &exp_var != observed {
                return Err(anyhow!(
                    "Value Incorrect for ({}): (Actual:{}, Expected:{})",
                    keys,
                    observed,
                    exp_var,
                ));
            }
        }
    }

    if discriminant(expected) != discriminant(observed) {
        return Err(anyhow!(
            "Expected type {:?} | Found type {:?}",
            expected.type_id(),
            observed.type_id()
        ));
    }

    if observed != expected {
        return Err(anyhow!(
            "Value Incorrect for ({}): (Actual:{}, Expected:{})",
            keys,
            observed,
            expected,
        ));
    }

    Err(anyhow!(
        "Value Incorrect for ({}): (Actual:{}, Expected:{})",
        keys,
        observed,
        expected,
    ))
}

pub fn compare_data_inner(
    observed: &Value,
    expected: &Value,
    full: bool,
    keys: String,
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
    compare_data_inner(
        observed,
        expected,
        full,
        "".to_string(),
        config,
        prior_steps,
    )
}

impl TestStepResult {
    pub fn make_failure(reason: TestStepFailureReason, message: String) -> TestStepResult {
        TestStepResult {
            status: reason,
            response_data: None,
            request_data: None,
            output_data: None,
            failure_message: Some(message),
        }
    }

    pub fn make_success(
        response_data: Value,
        request_data: Value,
        output_data: Value,
    ) -> TestStepResult {
        TestStepResult {
            status: TestStepFailureReason::NoFailure,
            response_data: Some(response_data),
            request_data: Some(request_data),
            output_data: Some(output_data),
            failure_message: None,
        }
    }

    pub fn get_field(&self, keys: String) -> Result<Option<Value>> {
        let sections: Vec<&str> = keys.split(".").collect();

        let mut first = true;
        let mut return_value: Option<Value> = None;

        if self.output_data.is_some() {
            return_value = self.output_data.clone();
            first = false;
        }

        for section in sections.iter() {
            if first {
                if *section == "response" {
                    return_value = self.response_data.clone();
                } else if *section == "request" || *section == "data" {
                    return_value = self.request_data.clone();
                } else if *section == "output" {
                    return_value = self.output_data.clone();
                } else {
                    return Err(anyhow!("Section {} not found in step", section));
                }
                first = false;
            } else {
                println!("{}, ret_val {:?}", section, return_value.clone());
                if let Some(new_val) = return_value.clone() {
                    println!("Is Some");
                    if let Some(obj_val) = new_val.as_object() {
                        println!("Is Obj");
                        if let Some(new) = obj_val.get(*section) {
                            println!("Got Val {}", new.clone());
                            return_value = Some(new.clone());
                        }
                    }
                }
            }
        }
        println!("Returning {:?}", return_value.clone());
        Ok(return_value.clone())
    }
}

impl TestStep {
    fn check_status_code(exp: Value, actual: u16) -> bool {
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
        return false;
    }

    fn get_identifier(&self, num_prior_steps: usize) -> String {
        match &self.id {
            Some(id) => id.clone(),
            None => num_prior_steps.to_string(),
        }
    }

    fn get_url(&self, config: &Option<Arc<RwLock<ConfigData>>>) -> Result<String> {
        // If URL is defined, return it
        if let Some(url_val) = &self.url {
            if url_val.starts_with("$") {
                let mut config_key = url_val.clone();
                config_key.remove(0);
                if let Some(cfg) = config {
                    return cfg.read().unwrap().get_string_value(config_key);
                }
            } else {
                return Ok(url_val.clone());
            }
        }
        if let Some(cfg) = config {
            return cfg
                .read()
                .unwrap()
                .get_string_value("urls.base".to_string());
        }

        return Err(anyhow!("Url not found"));
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
            return Method::GET;
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
            status: TestStepStatus::NotRun,
            failure_reason: TestStepFailureReason::NoFailure,
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
    fn get_status(&self) -> TestStepStatus;
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

        let mut url: String = "".to_string();

        match self.get_url(config) {
            Ok(actual_url) => {
                url = actual_url;
            }
            Err(e) => {
                let identifier = self.get_identifier(prior_steps.len());
                return Ok(TestStepResult::make_failure(
                    TestStepFailureReason::ConfigurationError,
                    format!("No url specified for step {}", identifier),
                ));
            }
        }

        match url.chars().last() {
            Some(last_char) => {
                if last_char == '/' {
                    url.pop();
                }
            }
            None => {}
        }

        let mut path = self.path.clone();
        match self.path.chars().next() {
            Some(first_char) => {
                if first_char != '/' {
                    path.insert(0, '/');
                }
            }
            None => {}
        }

        match clean_path(path, config, prior_steps) {
            Ok(new_path) => {
                path = new_path;
            }
            Err(e) => {
                return Err(anyhow!("Error generating path: {}", e));
            }
        }

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
                // Check if Status Code is correct
                if let Some(exp_status_code) = &self.expected_status_code {
                    let actual_status_code = response.status().as_u16();
                    if !TestStep::check_status_code(exp_status_code.clone(), actual_status_code) {
                        let failure_message = format!(
                            "Status Code incorrect. (Actual:{}, Expected:{})",
                            exp_status_code, actual_status_code,
                        );
                        return Ok(TestStepResult::make_failure(
                            TestStepFailureReason::StatusCodeError,
                            failure_message,
                        ));
                    }
                }

                match response.json::<Value>().await {
                    Ok(actual_response) => {
                        if let Some(expected_response) = self.expected_response_data.clone() {
                            if let Err(e) = compare_data(
                                &actual_response,
                                &expected_response,
                                config,
                                prior_steps,
                                !self.allow_missing_fields,
                            ) {
                                let failure_message = format!("Assertion Error: {}", e);
                                return Ok(TestStepResult::make_failure(
                                    TestStepFailureReason::ResponseError,
                                    failure_message,
                                ));
                            }
                        }
                        response_data = Some(actual_response.clone());
                    }
                    Err(e) => {
                        let failure_message = format!("Error Decoding Json: {}", e);
                        return Ok(TestStepResult::make_failure(
                            TestStepFailureReason::JsonDecodeError,
                            failure_message,
                        ));
                    }
                }
            }
            Err(e) => {
                return Err(anyhow!("Error Sending Request: {}", e));
            }
        }
        return Ok(TestStepResult {
            status: TestStepFailureReason::NoFailure,
            failure_message: None,
            request_data: Some(req_data),
            response_data,
            output_data: None,
        });
    }

    fn get_status(&self) -> TestStepStatus {
        self.status
    }
}
