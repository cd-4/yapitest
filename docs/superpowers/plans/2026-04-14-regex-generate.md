# Regex Generate & Match Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `re/<pattern>` syntax so string values in `data` blocks generate a matching string at request time, and string values in `assert.body` blocks validate the observed response against the regex.

**Architecture:** Both changes are additions within `rs/src/test_step.rs`. `clean_request_data` gains a `re/` prefix check that calls `regex_generate` before the existing `$` variable check. `compare_primitive_values` gains a `re/` branch after the `+type` check. Generated values are stored in `request_data` on the step result and are therefore accessible via `$step-id.data.field` in later steps.

**Tech Stack:** Rust, `regex_generate = "0.2.3"` (already in `Cargo.toml`), `regex = "1.12.3"` (already in `Cargo.toml`)

---

## File Map

- **Modify:** `rs/src/test_step.rs`
  - Add `re/` generation branch in `clean_request_data`
  - Add `re/` match branch in `compare_primitive_values`
  - Add `#[cfg(test)]` module with 7 test cases

---

### Task 1: Add `re/` generation to `clean_request_data`

**Files:**
- Modify: `rs/src/test_step.rs`

- [ ] **Step 1: Add the `re/` generation branch**

In `clean_request_data`, in the `else if let Some(data_str) = request_data.as_str()` arm, add the `re/` check **before** the existing `$` check:

```rust
} else if let Some(data_str) = request_data.as_str() {
    if let Some(pattern) = data_str.strip_prefix("re/") {
        use regex_generate::{DEFAULT_MAX_REPEAT, Generator};
        let mut gen = Generator::new(pattern, rand::thread_rng(), DEFAULT_MAX_REPEAT)
            .map_err(|e| anyhow!("invalid regex pattern 're/{}': {}", pattern, e))?;
        let mut buffer = vec![];
        gen.generate(&mut buffer)
            .map_err(|e| anyhow!("failed to generate string for 're/{}': {}", pattern, e))?;
        let generated = String::from_utf8(buffer)
            .map_err(|e| anyhow!("generated string for 're/{}' is not valid UTF-8: {}", pattern, e))?;
        Ok(Value::String(generated))
    } else if data_str.starts_with('$') {
        get_variable(data_str, config, prior_steps)
    } else {
        Ok(Value::from(data_str))
    }
```

Also add `rand` to the `use` imports at the top of `test_step.rs`:

```rust
use rand;
```

- [ ] **Step 2: Add `rand` to `Cargo.toml`**

In `rs/Cargo.toml`, add to `[dependencies]`:

```toml
rand = "0.8"
```

- [ ] **Step 3: Build to confirm it compiles**

```bash
cd rs && cargo build 2>&1
```

Expected: compiles with no errors (warnings about unused imports are fine at this stage).

- [ ] **Step 4: Commit**

```bash
git add rs/src/test_step.rs rs/Cargo.toml rs/Cargo.lock
git commit -m "feat: generate strings from re/ patterns in request data"
```

---

### Task 2: Add `re/` matching to `compare_primitive_values`

**Files:**
- Modify: `rs/src/test_step.rs`

- [ ] **Step 1: Add the `re/` assertion branch**

In `compare_primitive_values`, inside the `if let Some(exp_str) = expected.as_str()` block, add a new branch **after** the `+` type-check arm and **before** the `$` variable arm:

```rust
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
```

`Regex` is already imported at the top of `test_step.rs` (`use regex::Regex;`).

- [ ] **Step 2: Build to confirm it compiles**

```bash
cd rs && cargo build 2>&1
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add rs/src/test_step.rs
git commit -m "feat: match re/ patterns in assert.body assertions"
```

---

### Task 3: Write and run unit tests

**Files:**
- Modify: `rs/src/test_step.rs` (append `#[cfg(test)]` module)

- [ ] **Step 1: Write the failing tests**

Append the following to the bottom of `rs/src/test_step.rs`:

```rust
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
}
```

- [ ] **Step 2: Run tests to verify they fail (before implementation is complete)**

At this point Tasks 1 and 2 are already done, so tests should **pass**. Run them to confirm:

```bash
cd rs && cargo test 2>&1
```

Expected output (all 7 pass):
```
test tests::test_re_assert_fails_for_non_string_observed ... ok
test tests::test_re_assert_fails_on_no_match ... ok
test tests::test_re_assert_invalid_pattern_fails ... ok
test tests::test_re_assert_passes_on_match ... ok
test tests::test_re_generate_invalid_pattern_errors ... ok
test tests::test_re_generate_not_literal ... ok
test tests::test_re_generate_produces_match ... ok

test result: ok. 7 passed; 0 failed
```

- [ ] **Step 3: Commit**

```bash
git add rs/src/test_step.rs
git commit -m "test: add unit tests for re/ generate and match"
```
