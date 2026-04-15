# Duration Assertion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `duration` field to the `assert` block that validates the HTTP round-trip completed in less than the specified time.

**Architecture:** All changes are confined to `rs/src/test_step.rs`. `TestStep` stores the raw `Option<Value>` from the YAML; `from_spec` stays infallible. `parse_duration` is called at assertion time inside `run()` — a bad value surfaces as a failed `AssertionResult` rather than a load-time error. The measurement wraps `send().await` + `response.text().await`. Duration is pushed as a normal `AssertionResult` (never causes early exit on its own).

**Tech Stack:** Rust, `std::time::Instant`, `serde_json::Value` (already in use)

---

## File Map

- **Modify:** `rs/src/test_step.rs` — add `duration` field, `parse_duration`, timing in `run()`, unit tests

---

### Task 1: Add `parse_duration` with unit tests (TDD)

**Files:**
- Modify: `rs/src/test_step.rs`

- [ ] **Step 1: Add `duration` field to `TestStepAssertionSpec`**

In `test_step.rs`, change `TestStepAssertionSpec`:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestStepAssertionSpec {
    status_code: Option<Value>,
    body: Option<Value>,
    full: Option<bool>,
    duration: Option<Value>,
}
```

- [ ] **Step 2: Write the failing `parse_duration` unit tests**

Append these tests to the existing `#[cfg(test)] mod tests` block at the bottom of `test_step.rs` (inside the closing `}`):

```rust
    // ── parse_duration tests ─────────────────────────────────────────────────

    #[test]
    fn test_parse_duration_bare_int_value() {
        // YAML integer: duration: 500
        let result = parse_duration(&json!(500u64)).unwrap();
        assert_eq!(result, std::time::Duration::from_millis(500));
    }

    #[test]
    fn test_parse_duration_bare_int_string() {
        // YAML string: duration: "500"
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
        // "1.5s" — fractional seconds not supported
        let result = parse_duration(&json!("1.5s"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_duration_float_number_rejected() {
        // YAML float: duration: 1.5
        let result = parse_duration(&serde_json::json!(1.5_f64));
        assert!(result.is_err());
    }
```

- [ ] **Step 3: Run tests to confirm they fail**

```bash
cd rs && cargo test test_parse_duration 2>&1
```

Expected: compile error — `parse_duration` not yet defined. This confirms the tests are wired up.

- [ ] **Step 4: Implement `parse_duration`**

Add this free function to `test_step.rs`, before `clean_request_data`:

```rust
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
```

- [ ] **Step 5: Run tests to confirm they pass**

```bash
cd rs && cargo test test_parse_duration 2>&1
```

Expected: all 7 `test_parse_duration_*` tests pass.

- [ ] **Step 6: Commit**

```bash
git add rs/src/test_step.rs && git commit -m "feat: add parse_duration with unit tests"
```

---

### Task 2: Add `expected_duration` field to `TestStep` and populate in `from_spec`

**Files:**
- Modify: `rs/src/test_step.rs`

`from_spec` stays infallible. The raw `Option<Value>` is stored on the struct and parsed at assertion time in `run()`.

- [ ] **Step 1: Add `expected_duration: Option<Value>` to `TestStep` struct**

```rust
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
    expected_duration: Option<Value>,
}
```

- [ ] **Step 2: Populate `expected_duration` in `from_spec`**

Add `let mut expected_duration: Option<Value> = None;` before the `if let Some(assertion_data)` block, and set it inside:

```rust
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
    let mut expected_duration: Option<Value> = None;
    if let Some(assertion_data) = spec.assert {
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
        expected_response_data,
        expected_status_code,
        allow_missing_fields: !full_data,
        expected_duration,
    }
}
```

- [ ] **Step 3: Build to confirm it compiles**

```bash
cd rs && cargo build 2>&1
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add rs/src/test_step.rs && git commit -m "feat: store raw duration value on TestStep for deferred parsing"
```

---

### Task 3: Measure elapsed time and push duration assertion in `run()`

**Files:**
- Modify: `rs/src/test_step.rs`

- [ ] **Step 1: Add `push_duration_assertion` helper function**

Add this free function to `test_step.rs`, immediately before `impl TestStepResult`:

```rust
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
```

- [ ] **Step 2: Add timing and duration assertions in `TestStep::run()`**

In `TestStep::run()`, find this block:

```rust
        match client
            .request(self.method.clone(), full_url)
            .headers(headers)
            .json(&req_data)
            .send()
            .await
        {
            Ok(response) => {
                let actual_status_code = response.status().as_u16();

                if let Some(exp_status_code) = &self.expected_status_code {
                    let passed = TestStep::check_status_code(exp_status_code, actual_status_code);
                    assertions.push(AssertionResult {
                        name: format!("status {}", exp_status_code),
                        passed,
                        message: if passed { None } else {
                            Some(format!("expected status {}, got {}", exp_status_code, actual_status_code))
                        },
                    });
                    if !passed {
                        let msg = assertions.last().unwrap().message.clone().unwrap_or_default();
                        return Ok(TestStepResult {
                            step_id: self.id.clone(),
                            status: TestStepFailureReason::StatusCodeError,
                            failure_message: Some(msg),
                            response_data: None,
                            request_data: Some(req_data),
                            output_data: None,
                            assertion_results: assertions,
                        });
                    }
                }

                let res_text = response.text().await?;

                match serde_json::from_str::<Value>(&res_text) {
                    Ok(actual_response) => {
                        if let Some(expected_response) = &self.expected_response_data {
                            let all_passed = compare_data(
                                &actual_response,
                                expected_response,
                                config,
                                prior_steps,
                                !self.allow_missing_fields,
                                &mut assertions,
                            );
                            if !all_passed {
                                let msg = assertions.iter()
                                    .find(|a| !a.passed)
                                    .and_then(|a| a.message.clone())
                                    .unwrap_or_default();
                                return Ok(TestStepResult {
                                    step_id: self.id.clone(),
                                    status: TestStepFailureReason::ResponseError,
                                    failure_message: Some(msg),
                                    response_data: Some(actual_response),
                                    request_data: Some(req_data),
                                    output_data: None,
                                    assertion_results: assertions,
                                });
                            }
                        }
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
```

Replace it with:

```rust
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
                let res_text = response.text().await?;
                let elapsed = t0.elapsed();

                if let Some(exp_status_code) = &self.expected_status_code {
                    let passed = TestStep::check_status_code(exp_status_code, actual_status_code);
                    assertions.push(AssertionResult {
                        name: format!("status {}", exp_status_code),
                        passed,
                        message: if passed { None } else {
                            Some(format!("expected status {}, got {}", exp_status_code, actual_status_code))
                        },
                    });
                    if !passed {
                        let msg = assertions.last().unwrap().message.clone().unwrap_or_default();
                        push_duration_assertion(&mut assertions, self.expected_duration.as_ref(), elapsed);
                        return Ok(TestStepResult {
                            step_id: self.id.clone(),
                            status: TestStepFailureReason::StatusCodeError,
                            failure_message: Some(msg),
                            response_data: None,
                            request_data: Some(req_data),
                            output_data: None,
                            assertion_results: assertions,
                        });
                    }
                }

                match serde_json::from_str::<Value>(&res_text) {
                    Ok(actual_response) => {
                        if let Some(expected_response) = &self.expected_response_data {
                            let all_passed = compare_data(
                                &actual_response,
                                expected_response,
                                config,
                                prior_steps,
                                !self.allow_missing_fields,
                                &mut assertions,
                            );
                            push_duration_assertion(&mut assertions, self.expected_duration.as_ref(), elapsed);
                            if !all_passed {
                                let msg = assertions.iter()
                                    .find(|a| !a.passed)
                                    .and_then(|a| a.message.clone())
                                    .unwrap_or_default();
                                return Ok(TestStepResult {
                                    step_id: self.id.clone(),
                                    status: TestStepFailureReason::ResponseError,
                                    failure_message: Some(msg),
                                    response_data: Some(actual_response),
                                    request_data: Some(req_data),
                                    output_data: None,
                                    assertion_results: assertions,
                                });
                            }
                        } else {
                            push_duration_assertion(&mut assertions, self.expected_duration.as_ref(), elapsed);
                        }
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
```

- [ ] **Step 3: Build to confirm it compiles**

```bash
cd rs && cargo build 2>&1
```

Expected: no errors.

- [ ] **Step 4: Run all tests**

```bash
cd rs && cargo test 2>&1
```

Expected: all tests pass (7 existing + 7 new = 14 total).

- [ ] **Step 5: Commit**

```bash
git add rs/src/test_step.rs && git commit -m "feat: measure request duration and assert in assert.duration"
```
