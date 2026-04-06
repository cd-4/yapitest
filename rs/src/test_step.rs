use crate::config::ConfigData;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::{Client, Method};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::any::Any;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
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
    response_data: Option<Value>,
    request_data: Option<Value>,
    output_data: Option<Value>,
    pub status: TestStepFailureReason,
    pub failure_message: Option<String>,
}

pub fn get_variable(
    name: String,
    config: &ConfigData,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<Value> {
    if !name.starts_with('$') {
        return Ok(Value::String(name));
    }
    let mut current_key = name.clone();
    'outer: while current_key.starts_with('$') {
        let mut value_key = current_key.clone();
        value_key.remove(0);
        if let Ok(new_val) = config.get_string_value(value_key.clone()) {
            current_key = new_val;
        } else {
            for (step_id, step) in prior_steps.iter() {
                let mut new_key: String = step_id.clone();
                new_key.push('.');
                if new_key.starts_with(&new_key.clone()) {
                    let trimmed_str = value_key.strip_prefix(&new_key).unwrap();
                    match step.get_field(trimmed_str.to_string()) {
                        Ok(value) => {
                            if let Some(v) = value {
                                if let Some(string_val) = v.as_str() {
                                    if string_val.starts_with('$') {
                                        continue 'outer;
                                    }
                                }
                                return Ok(v);
                            } else {
                                return Err(anyhow!("Value not found: {}", name));
                            }
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            }
        }
    }
    return Err(anyhow!("Value not found: {}", name));
}

pub fn clean_data(
    value: &Value,
    config: &ConfigData,
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

/*
    fn clean_expected_response(
        &self,
        config: &ConfigData,
        expected_response: &Value,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> Result<Value> {
        let mut clone_res = expected_response.clone();
        if let Some(ref mut map) = clone_res.as_object_mut() {
            let keys: Vec<String> = map.iter().filter_map(|(k, _)| Some(k.clone())).collect();

            for k in keys.iter() {
                if let Some(value) = map.get_mut(k) {
                    match self.clean_expected_response(&config, value, prior_steps) {
                        Ok(new_value) => {
                            map.insert(k.clone(), new_value);
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            }
            return Ok(Value::Object(map.clone()));
        } else if let Some(ref mut vec) = clone_res.as_array_mut() {
            // Build a completely new vector from the cleaned items
            let mut cleaned_vec: Vec<Value> = Vec::with_capacity(vec.len());

            for item in vec.iter_mut() {
                let cleaned_item = self.clean_expected_response(config, item, prior_steps)?;
                cleaned_vec.push(cleaned_item);
            }

            return Ok(Value::Array(cleaned_vec));
        } else if let Some(str) = expected_response.as_str() {
            if str.starts_with('$') {
                let mut config_key = str.to_string();
                config_key.remove(0); // remove leading $

                if let Ok(new_value) = config.get_string_value(config_key.clone()) {
                    return Ok(Value::String(new_value));
                }

                for (_step_id, step) in prior_steps.iter() {
                    if let Ok(result) = step.get_field(config_key.clone()) {
                        if let Some(res) = result {
                            return Ok(res.clone());
                        } else {
                            continue;
                        }
                    }
                }
                return Err(anyhow!("Key {} not found", str));
            } else {
                return Ok(expected_response.clone());
            }
        } else {
            return Ok(expected_response.clone());
        }
    }
*/

pub fn check_size(val: &Value, size_str: String) -> bool {
    return false;
}

pub fn compare_data_objects(
    observed_object: &Map<String, Value>,
    expected_object: &Map<String, Value>,
    full: bool,
    keys: String,
) -> Result<()> {
    for key in observed_object.keys() {
        let observed = observed_object.get(key).unwrap();
        let exp_value = expected_object.get(key);

        if exp_value.is_none() {
            if full {
                return Err(anyhow!(
                    "'full' set and value '{}.{}' was not found",
                    keys,
                    key
                ));
            }
            continue;
        }

        let expected = exp_value.unwrap();

        compare_data_inner(observed, expected, full, format!("{}.{}", keys, key))?;
    }

    Ok(())
}

pub fn compare_array_objects(
    observed_object: &Vec<Value>,
    expected_object: &Vec<Value>,
    full: bool,
    keys: String,
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
        compare_data_inner(observed, expected, full, new_keys)?;
    }

    Ok(())
}

pub fn compare_primitive_values(observed: &Value, expected: &Value, keys: String) -> Result<()> {
    if let Some(exp_str) = expected.as_str() {
        if exp_str.starts_with('+') {
            let mut exp_type = exp_str.to_string();
            exp_type.remove(0);
            if (exp_type == "str" || exp_type == "string") && observed.as_str().is_none() {
                return Err(anyhow!("Expected string for {}", keys));
            } else if (exp_type == "float" || exp_type == "flt") && observed.as_f64().is_none() {
                return Err(anyhow!("Expected float for {}", keys));
            } else if exp_type == "int" && observed.as_i64().is_none() {
                return Err(anyhow!("Expected int for {}", keys));
            }
        } else if exp_str.starts_with("len") {
            let size_str = exp_str.strip_prefix("len").unwrap();
            if !check_size(observed, size_str.to_string()) {
                return Err(anyhow!("Size Incorrect TODO"));
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
            "For key: {} Expected: {} | Value Found {}",
            keys,
            expected,
            observed
        ));
    }

    Err(anyhow!("Expected: {} | Value Found {}", expected, observed))
}

pub fn compare_data_inner(
    observed: &Value,
    expected: &Value,
    full: bool,
    keys: String,
) -> Result<()> {
    if let (Some(obs_obj), Some(exp_obj)) = (observed.as_object(), expected.as_object()) {
        compare_data_objects(obs_obj, exp_obj, full, keys)
    } else if let (Some(obs_arr), Some(exp_arr)) = (observed.as_array(), expected.as_array()) {
        compare_array_objects(obs_arr, exp_arr, full, keys)
    } else {
        compare_primitive_values(observed, expected, keys)
    }
}

pub fn compare_data(
    observed: &Value,
    expected: &Value,
    config: &ConfigData,
    prior_steps: &HashMap<String, TestStepResult>,
    full: bool,
) -> Result<()> {
    match clean_data(expected, config, prior_steps) {
        Ok(exp) => compare_data_inner(observed, &exp, full, "".to_string()),
        Err(e) => Err(anyhow!("Error cleaning expected data {}", e)),
    }
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
        for section in sections.iter() {
            if first {
                if *section == "response" {
                    return_value = self.response_data.clone();
                } else if *section == "request" {
                    return_value = self.request_data.clone();
                } else if *section == "output" {
                    return_value = self.output_data.clone();
                } else {
                    return Err(anyhow!("Section {} not found in step", section));
                }
                first = false;
            } else {
                if let Some(new_val) = return_value.clone() {
                    if let Some(obj_val) = new_val.as_object() {
                        if let Some(new) = obj_val.get(*section) {
                            return_value = Some(new.clone());
                        }
                    }
                }
            }
        }
        Ok(return_value.clone())
    }
}

impl TestStep {
    fn get_expected_response(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        expected_res: &Value,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> Result<Value> {
        if let Some(cfg) = config {
            return self.get_expected_response_inner(
                &cfg.read().unwrap(),
                expected_res,
                prior_steps,
            );
        } else {
            return Ok(expected_res.clone());
        }
    }

    fn get_expected_response_inner(
        &self,
        config: &ConfigData,
        expected_res: &Value,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> Result<Value> {
        match self.clean_expected_response(config, expected_res, prior_steps) {
            Ok(response) => {
                return Ok(response);
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    fn clean_expected_response(
        &self,
        config: &ConfigData,
        expected_response: &Value,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> Result<Value> {
        let mut clone_res = expected_response.clone();
        if let Some(ref mut map) = clone_res.as_object_mut() {
            let keys: Vec<String> = map.iter().filter_map(|(k, _)| Some(k.clone())).collect();

            for k in keys.iter() {
                if let Some(value) = map.get_mut(k) {
                    match self.clean_expected_response(&config, value, prior_steps) {
                        Ok(new_value) => {
                            map.insert(k.clone(), new_value);
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            }
            return Ok(Value::Object(map.clone()));
        } else if let Some(ref mut vec) = clone_res.as_array_mut() {
            // Build a completely new vector from the cleaned items
            let mut cleaned_vec: Vec<Value> = Vec::with_capacity(vec.len());

            for item in vec.iter_mut() {
                let cleaned_item = self.clean_expected_response(config, item, prior_steps)?;
                cleaned_vec.push(cleaned_item);
            }

            return Ok(Value::Array(cleaned_vec));
        } else if let Some(str) = expected_response.as_str() {
            if str.starts_with('$') {
                let mut config_key = str.to_string();
                config_key.remove(0); // remove leading $

                if let Ok(new_value) = config.get_string_value(config_key.clone()) {
                    return Ok(Value::String(new_value));
                }

                for (_step_id, step) in prior_steps.iter() {
                    if let Ok(result) = step.get_field(config_key.clone()) {
                        if let Some(res) = result {
                            return Ok(res.clone());
                        } else {
                            continue;
                        }
                    }
                }
                return Err(anyhow!("Key {} not found", str));
            } else {
                return Ok(expected_response.clone());
            }
        } else {
            return Ok(expected_response.clone());
        }
    }

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

    fn check_response(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        expected: &Value,
        actual: &Value,
        prior_steps: &HashMap<String, TestStepResult>,
        full: bool,
    ) -> Result<()> {
        let mut compare_mode = serde_json_assert::CompareMode::Inclusive;
        if full {
            compare_mode = serde_json_assert::CompareMode::Strict;
        }

        let assert_config =
            serde_json_assert::Config::new(compare_mode).consider_array_sorting(false);

        match self.get_expected_response(config, expected, prior_steps) {
            Ok(exp) => {
                match serde_json_assert::assert_json_matches_no_panic(actual, &exp, &assert_config)
                {
                    Ok(_res) => {
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(anyhow!(e));
                    }
                }
            }
            Err(e) => {
                return Err(anyhow!(e));
            }
        }
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

        let full_url = format!("{}{}", url, path);

        match client
            .request(self.method.clone(), full_url)
            .json(&self.request_data)
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

                if let Some(expected_response) = self.expected_response_data.clone() {
                    match response.json::<Value>().await {
                        Ok(actual_response) => {
                            match self.get_expected_response(
                                &config,
                                &expected_response,
                                prior_steps,
                            ) {
                                Ok(expected) => {
                                    if let Err(e) = self.check_response(
                                        &config,
                                        &expected,
                                        &actual_response,
                                        prior_steps,
                                        true,
                                    ) {
                                        let failure_message = format!("Response Incorrect: {}", e);
                                        return Ok(TestStepResult::make_failure(
                                            TestStepFailureReason::ResponseError,
                                            failure_message,
                                        ));
                                    }

                                    return Ok(TestStepResult {
                                        status: TestStepFailureReason::NoFailure,
                                        failure_message: None,
                                        request_data: Some(self.request_data.clone()),
                                        response_data: Some(actual_response),
                                        output_data: None,
                                    });
                                }
                                Err(e) => {
                                    let failure_message =
                                        format!("Unable to Decode Expected Response: {}", e);
                                    return Ok(TestStepResult::make_failure(
                                        TestStepFailureReason::ResponseError,
                                        failure_message,
                                    ));
                                }
                            }
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
            }
            Err(e) => {
                return Err(anyhow!("Error Sending Request: {}", e));
            }
        }
        return Ok(TestStepResult {
            status: TestStepFailureReason::NoFailure,
            failure_message: None,
            request_data: Some(self.request_data.clone()),
            response_data: None,
            output_data: None,
        });
    }

    fn get_status(&self) -> TestStepStatus {
        self.status
    }
}
