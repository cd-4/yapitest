use crate::config::ConfigData;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::{Client, Method};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::sync::{Arc, RwLock};

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
    status: TestStepFailureReason,
    failure_message: Option<String>,
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

    fn check_response(expected: &Value, actual: &Value, full: bool) -> Result<()> {
        let mut compare_mode = serde_json_assert::CompareMode::Inclusive;
        if full {
            compare_mode = serde_json_assert::CompareMode::Strict;
        }

        let config = serde_json_assert::Config::new(compare_mode).consider_array_sorting(false);
        match serde_json_assert::assert_json_matches_no_panic(actual, expected, &config) {
            Ok(()) => {}
            Err(e) => {
                return Err(anyhow!(e));
            }
        }
        Ok(())
    }

    fn get_identifier(&self, num_prior_steps: usize) -> String {
        match &self.id {
            Some(id) => id.clone(),
            None => num_prior_steps.to_string(),
        }
    }

    fn get_url(&self, config: Option<Arc<RwLock<ConfigData>>>) -> Result<String> {
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
        &mut self,
        config: Option<Arc<RwLock<ConfigData>>>,
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
        &mut self,
        config: Option<Arc<RwLock<ConfigData>>>,
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
                            exp_status_code.to_string(),
                            actual_status_code.to_string()
                        );
                        return Ok(TestStepResult::make_failure(
                            TestStepFailureReason::StatusCodeError,
                            failure_message,
                        ));
                    }
                }

                if let Some(expected_response) = &self.expected_response_data {
                    match response.json::<Value>().await {
                        Ok(actual_response) => {
                            if let Err(e) =
                                TestStep::check_response(&expected_response, &actual_response, true)
                            {
                                let failure_message = format!("Response Incorrect: {}", e);
                                return Ok(TestStepResult::make_failure(
                                    TestStepFailureReason::ResponseError,
                                    failure_message,
                                ));
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

        return Err(anyhow!("Temporary Error"));

        /*
        let res = client
            .post("https://api.example.com/login")
            .json(&payload)
            .send()
            .await?
            .json::<Value>() // ← gets serde_json::Value
            .await?;

        println!("Response:\n{:#}", res);

        // Access fields safely
        if let Some(token) = res["token"].as_str() {
            println!("Got token: {}", token);
        }
        */
    }

    fn get_status(&self) -> TestStepStatus {
        self.status
    }
}
