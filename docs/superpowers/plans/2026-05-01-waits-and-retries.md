# Waits and Retries Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `wait-before`, `wait-after`, and `retry` fields to test steps so users can control timing and retry on assertion failures.

**Architecture:** All changes live in `rs/src/test_step.rs`. New fields are added to `TestStepSpec` (YAML deserialization) and `TestStep` (runtime). The current `run()` body moves to a private `run_attempt()` method; `run()` gains wait-before sleep, a retry loop over `run_attempt()`, and wait-after sleep. Docs are updated in `Tests.md` and `docs/tests.md`.

**Tech Stack:** Rust, serde/serde_yaml (deserialization), tokio::time::sleep (async sleep), existing `parse_duration` helper.

---

## File Map

| File | Change |
|------|--------|
| `rs/src/test_step.rs` | Add fields to `TestStepSpec` and `TestStep`; update `from_spec()`; add `run_attempt()`; rewrite `run()` |
| `Tests.md` | Document `wait-before`, `wait-after`, `retry` under Step fields |
| `docs/tests.md` | Mirror the same additions |

---

### Task 1: Add failing deserialization tests

**Files:**
- Modify: `rs/src/test_step.rs` — append tests to the `#[cfg(test)]` block

- [ ] **Step 1: Add tests to the `#[cfg(test)]` block in `rs/src/test_step.rs`**

Append the following inside the `mod tests { ... }` block, after the last existing test (line ~1926):

```rust
// ── wait-before / wait-after / retry deserialization ────────────────────────

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
```

- [ ] **Step 2: Verify tests fail to compile**

```bash
cd rs && cargo test 2>&1 | head -30
```

Expected: compile error — `wait_before`, `wait_after`, `retry` fields not found on `TestStepSpec` or `TestStep`.

---

### Task 2: Add fields to structs and update `from_spec()`

**Files:**
- Modify: `rs/src/test_step.rs` — `TestStepSpec`, `TestStep`, `TestStep::from_spec()`

- [ ] **Step 1: Add three fields to `TestStepSpec`**

Locate the `TestStepSpec` struct (lines 42–52). Replace it with:

```rust
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
```

- [ ] **Step 2: Add three fields to `TestStep`**

Locate the `TestStep` struct (lines 54–65). Replace it with:

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
    wait_before: Option<Value>,
    wait_after: Option<Value>,
    retry: u32,
}
```

- [ ] **Step 3: Update `TestStep::from_spec()` to populate new fields**

Locate the `TestStep { ... }` struct literal at the end of `from_spec()` (lines 868–880). Replace it with:

```rust
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
            wait_before: spec.wait_before,
            wait_after: spec.wait_after,
            retry: spec.retry.unwrap_or(0),
        }
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd rs && cargo test 2>&1 | tail -20
```

Expected: all new tests pass, all existing tests still pass. Zero failures.

- [ ] **Step 5: Commit**

```bash
cd rs && git add src/test_step.rs && git commit -m "$(cat <<'EOF'
Add wait-before, wait-after, retry fields to TestStep

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Extract `run_attempt()` and update `run()`

**Files:**
- Modify: `rs/src/test_step.rs` — add `run_attempt()` to `impl TestStep`; rewrite `run()` in `impl RunnableTestStep for TestStep`

- [ ] **Step 1: Add `run_attempt()` to `impl TestStep`**

Locate the `impl TestStep { ... }` block (the one containing `from_spec`, `check_status_code`, `get_url`, `get_method`). Add `run_attempt` as a new method at the end of that block, before its closing `}`:

```rust
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
                        message: if passed {
                            None
                        } else {
                            Some(format!(
                                "expected status {}, got {}",
                                exp_status_code, actual_status_code
                            ))
                        },
                    });
                    if !passed {
                        let msg = assertions
                            .last()
                            .unwrap()
                            .message
                            .clone()
                            .unwrap_or_default();
                        push_duration_assertion(
                            &mut assertions,
                            self.expected_duration.as_ref(),
                            elapsed,
                        );
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
                            push_duration_assertion(
                                &mut assertions,
                                self.expected_duration.as_ref(),
                                elapsed,
                            );
                            if !all_passed {
                                let msg = assertions
                                    .iter()
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
                            push_duration_assertion(
                                &mut assertions,
                                self.expected_duration.as_ref(),
                                elapsed,
                            );
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
```

- [ ] **Step 2: Replace `run()` in `impl RunnableTestStep for TestStep`**

Locate the `async fn run(...)` inside `#[async_trait] impl RunnableTestStep for TestStep { ... }` (lines 899–1037). Replace the entire body with:

```rust
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
```

- [ ] **Step 3: Verify it compiles and all tests still pass**

```bash
cd rs && cargo test 2>&1 | tail -20
```

Expected: all tests pass, zero failures, zero warnings about unused fields.

- [ ] **Step 4: Commit**

```bash
cd rs && git add src/test_step.rs && git commit -m "$(cat <<'EOF'
Extract run_attempt, add wait-before/wait-after/retry to run

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Update `Tests.md`

**Files:**
- Modify: `Tests.md` — add `wait-before`, `wait-after`, `retry` entries under Step fields

- [ ] **Step 1: Add new step field docs to `Tests.md`**

Locate the `#### assert *(optional)*` section (around line 219). After the `assert` block and before the `---` separator that leads into `## Variables`, insert:

```markdown
#### `wait-before` *(optional)*

Duration to sleep before the step runs. Accepts a bare integer (milliseconds), or a string with a `ms` or `s` suffix. Applies once before the first attempt — does **not** repeat between retries.

```yaml
wait-before: 500       # 500 milliseconds
wait-before: "500ms"   # same
wait-before: "2s"      # 2 seconds
```

#### `wait-after` *(optional)*

Duration to sleep after the step completes — whether it passed or failed (after all retries). Same format as `wait-before`. Useful for rate-limiting or letting downstream state propagate before the next step runs.

```yaml
wait-after: 1s
```

#### `retry` *(optional)*

Number of additional attempts to make if the step's assertions fail. Default: `0` (no retries). On each attempt the full HTTP request and all assertions are re-run. `wait-before` and `wait-after` do not repeat between attempts. If all attempts fail, the test stops immediately with the last failure.

```yaml
retry: 3    # up to 4 total attempts (1 initial + 3 retries)
```

```yaml
# Combining wait and retry: poll a slow endpoint
- path: /api/job/$create-job.response.id/status
  wait-before: 2s     # give the job time to start
  retry: 5            # retry up to 5 times if status assertion fails
  wait-after: 500ms   # brief pause before the next step
  assert:
    status-code: 200
    body:
      status: "complete"
```
```

- [ ] **Step 2: Verify the file looks correct**

```bash
grep -n "wait-before\|wait-after\|retry" Tests.md
```

Expected: at least 6 matches showing the new headings and YAML examples.

- [ ] **Step 3: Commit**

```bash
git add Tests.md && git commit -m "$(cat <<'EOF'
Document wait-before, wait-after, retry in Tests.md

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Update `docs/tests.md`

**Files:**
- Modify: `docs/tests.md` — mirror the same additions from Task 4

- [ ] **Step 1: Add new step field docs to `docs/tests.md`**

Locate the `#### assert *(optional)*` section (around line 209). After the `assert` block and before the `---` separator, insert the identical content as Task 4 Step 1 above:

```markdown
#### `wait-before` *(optional)*

Duration to sleep before the step runs. Accepts a bare integer (milliseconds), or a string with a `ms` or `s` suffix. Applies once before the first attempt — does **not** repeat between retries.

```yaml
wait-before: 500       # 500 milliseconds
wait-before: "500ms"   # same
wait-before: "2s"      # 2 seconds
```

#### `wait-after` *(optional)*

Duration to sleep after the step completes — whether it passed or failed (after all retries). Same format as `wait-before`. Useful for rate-limiting or letting downstream state propagate before the next step runs.

```yaml
wait-after: 1s
```

#### `retry` *(optional)*

Number of additional attempts to make if the step's assertions fail. Default: `0` (no retries). On each attempt the full HTTP request and all assertions are re-run. `wait-before` and `wait-after` do not repeat between attempts. If all attempts fail, the test stops immediately with the last failure.

```yaml
retry: 3    # up to 4 total attempts (1 initial + 3 retries)
```

```yaml
# Combining wait and retry: poll a slow endpoint
- path: /api/job/$create-job.response.id/status
  wait-before: 2s     # give the job time to start
  retry: 5            # retry up to 5 times if status assertion fails
  wait-after: 500ms   # brief pause before the next step
  assert:
    status-code: 200
    body:
      status: "complete"
```
```

- [ ] **Step 2: Verify the file looks correct**

```bash
grep -n "wait-before\|wait-after\|retry" docs/tests.md
```

Expected: at least 6 matches.

- [ ] **Step 3: Commit**

```bash
git add docs/tests.md && git commit -m "$(cat <<'EOF'
Document wait-before, wait-after, retry in docs/tests.md

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review

**Spec coverage:**
- ✅ `wait-before` field: Task 2 (struct), Task 3 (run logic), Tasks 4–5 (docs)
- ✅ `wait-after` field: Task 2 (struct), Task 3 (run logic), Tasks 4–5 (docs)
- ✅ `retry` field: Task 2 (struct), Task 3 (run logic), Tasks 4–5 (docs)
- ✅ Duration format same as `assert.duration` (reuses `parse_duration`)
- ✅ `retry` default 0: `spec.retry.unwrap_or(0)` in `from_spec()`
- ✅ `wait-before` applies once before first attempt (sleep is outside retry loop)
- ✅ `wait-after` applies after final result (sleep is outside retry loop)
- ✅ Tests for deserialization: Task 1
- ✅ Tests for field propagation through `from_spec()`: Task 1
- ✅ Both docs files updated: Tasks 4 and 5

**Placeholder scan:** No TBDs, no "implement later", no vague steps. All code blocks are complete.

**Type consistency:** `wait_before: Option<Value>` and `wait_after: Option<Value>` used consistently in `TestStepSpec`, `TestStep`, `from_spec()`, and `run()`. `retry: u32` used consistently throughout. `parse_duration` signature `(v: &Value) -> Result<std::time::Duration>` is unchanged.
