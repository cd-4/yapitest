use crate::config::ConfigData;
use reqwest::{Client, Method};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};
use std::str::FromStr;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TestStepFailureReason {
    NoFailure,
    NoResponse,
    ResponseError,
    StatusCodeError,
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
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
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
    status: TestStepFailureReason,
    failure_message: Option<String>,
}

impl TestStepResult {
    pub fn make_failure(reason: TestStepFailureReason, message: String) -> TestStepResult {
        TestStepResult {
            status: reason,
            response_data: None,
            failure_message: Some(message),
        }
    }

    pub fn make_success(response_data: Value) -> TestStepResult {
        TestStepResult {
            status: TestStepFailureReason::NoFailure,
            response_data: Some(response_data),
            failure_message: None,
        }
    }
}

impl TestStep {
    fn get_url(&self, config: Option<ConfigData>) -> String {
        // If URL is defined, return it
        if let Some(url_val) = self.url {
            if url_val.starts_with("$") {
                let mut config_key = url_val.copy();
                config_key.remove(0);
                if let Some(config) = config {
                    return config.get_string_value(key);
                }
            } else {
                return url_val;
            }
        }
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

pub trait RunnableTestStep {
    fn get_id(&self) -> Option<&String>;
    fn run(
        &mut self,
        config: Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> TestStepResult;
    fn get_status(&self) -> TestStepStatus;
}

impl RunnableTestStep for TestStep {
    fn get_id(&self) -> Option<&String> {
        self.id.as_ref()
    }

    fn run(
        &mut self,
        config: Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> TestStepResult {
        /*
        let client = Client::new();

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
