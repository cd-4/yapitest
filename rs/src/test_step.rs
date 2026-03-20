use reqwest::{Client, Method};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};

use crate::config::TestStepGroup;

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

struct TestStep {
    id: Option<String>,
    path: String,
    method: Option<Method>,
    header_data: HashMap<String, String>,
    request_data: Value,
    expected_response_data: Value,
    allow_missing_fields: bool,
    assert_data: Value,
    status: TestStepStatus,
    failure_reason: TestStepFailureReason,
}

pub trait RunnableTestStep {
    fn get_id(&self) -> Option<&String>;
    fn run(&mut self);
    fn get_status(&self) -> TestStepStatus;
}

impl RunnableTestStep for TestStep {
    fn get_id(&self) -> Option<&String> {
        self.id.as_ref()
    }

    fn run(&mut self) {
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
