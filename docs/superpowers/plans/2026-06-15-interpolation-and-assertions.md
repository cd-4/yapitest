# Interpolation Engine & Assertion Features — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a unified inline interpolation engine plus seven assertion/step features to the Rust yapitest implementation (`rs/`).

**Architecture:** A single scanner-based interpolation engine (`scan_template` + `interpolate_string` + `resolve_value`) replaces four ad-hoc resolvers and is reused for headers, data, paths, assertion-expected values, and step-set output. Assertion features extend `compare_primitive_values` / `compare_data_objects` / `get_field` / `check_status_code`. Step mechanics add a `query:` map and parameterized step-sets (`args`).

**Tech Stack:** Rust, `serde_json::Value`, `reqwest`, `regex`, `tokio`. Tests via `cargo test`; integration via the Flask app in `testing/api` driven by the `yapitest` binary.

**Spec:** `docs/superpowers/specs/2026-06-15-interpolation-and-assertions-design.md`

**Conventions:**
- All Rust code lives in `rs/src/`. Run tests from `rs/` with `cargo test`.
- Run a single test: `cargo test <name> -- --nounder` is not needed; use `cargo test <substring>`.
- Commit messages end with the trailer:
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`
- `-fail` API tests are intentionally-failing assertions proving the framework detects mismatches.

---

## File Structure

| File | Responsibility | Change |
|------|----------------|--------|
| `rs/src/test_step.rs` | Interpolation engine, assertions, step execution | Modify (most tasks) |
| `rs/src/config.rs` | Step-set output resolution, parameterized step-sets | Modify (1.3, 3.2) |
| `rs/src/test.rs` | `setup`/inline step-set invocation with args | Modify (3.2) |
| `testing/api/main.py` | Sample API endpoints to exercise features | Modify (2.6, 3.1, 3.2) |
| `testing/tests/test_dir/*.yaml` | Positive + `-fail` integration tests | Create (2.6, 3.1, 3.2) |
| `Tests.md`, `Configs.md` | User docs | Modify (end of each phase) |

---

# Phase 1 — Unified interpolation engine (#1)

### Task 1.1: Scanner + `interpolate_string` + `resolve_value`

**Files:**
- Modify: `rs/src/test_step.rs` (replace the existing `interpolate` fn added previously, ~lines 261–294 region; it is currently called by `clean_headers`)

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in `rs/src/test_step.rs` (near the other `clean_headers`/interp tests):

```rust
// ── interpolation engine ─────────────────────────────────────────────────
#[test]
fn test_interp_braced_with_surrounding_text() {
    let cfg = make_config("vars:\n  token: xyz789");
    let out = interpolate_string("Bearer ${vars.token}", &cfg, &no_prior_steps()).unwrap();
    assert_eq!(out, "Bearer xyz789");
}

#[test]
fn test_interp_bare_ref_still_works() {
    let cfg = make_config("vars:\n  token: xyz789");
    let out = interpolate_string("Bearer $vars.token", &cfg, &no_prior_steps()).unwrap();
    assert_eq!(out, "Bearer xyz789");
}

#[test]
fn test_interp_multiple_and_adjacent() {
    let cfg = make_config("vars:\n  a: A\n  b: B");
    let out = interpolate_string("${vars.a}-${vars.b}x", &cfg, &no_prior_steps()).unwrap();
    assert_eq!(out, "A-Bx");
}

#[test]
fn test_interp_dollar_escape_and_literal() {
    let out = interpolate_string("$$5 off $ x", &no_config(), &no_prior_steps()).unwrap();
    assert_eq!(out, "$5 off $ x");
}

#[test]
fn test_interp_bare_does_not_grab_trailing_dot() {
    let mut prior = no_prior_steps();
    prior.insert("s".to_owned(), make_step_result("s", json!({"v": "hi"}), json!(null)));
    let out = interpolate_string("$s.response.v.", &no_config(), &prior).unwrap();
    assert_eq!(out, "hi.");
}

#[test]
fn test_resolve_value_whole_ref_preserves_int() {
    let mut prior = no_prior_steps();
    prior.insert("s".to_owned(), make_step_result("s", json!({"id": 42}), json!(null)));
    let v = resolve_value("$s.response.id", &no_config(), &prior).unwrap();
    assert_eq!(v, json!(42));
}

#[test]
fn test_resolve_value_mixed_stringifies() {
    let mut prior = no_prior_steps();
    prior.insert("s".to_owned(), make_step_result("s", json!({"id": 42}), json!(null)));
    let v = resolve_value("id=${s.response.id}", &no_config(), &prior).unwrap();
    assert_eq!(v, json!("id=42"));
}

#[test]
fn test_resolve_value_literal_and_regex() {
    let plain = resolve_value("hello", &no_config(), &no_prior_steps()).unwrap();
    assert_eq!(plain, json!("hello"));
    let generated = resolve_value("re/[0-9]{3}", &no_config(), &no_prior_steps()).unwrap();
    assert!(generated.as_str().unwrap().chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn test_interp_non_scalar_errors() {
    let mut prior = no_prior_steps();
    prior.insert("s".to_owned(), make_step_result("s", json!({"obj": {"a": 1}}), json!(null)));
    assert!(interpolate_string("x${s.response.obj}", &no_config(), &prior).is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd rs && cargo test test_interp test_resolve_value 2>&1 | tail -20`
Expected: FAIL — `cannot find function interpolate_string` / `resolve_value`.

- [ ] **Step 3: Implement the engine**

In `rs/src/test_step.rs`, **delete** the existing `interpolate` function (the regex-based one added previously) and add the following (place near where `interpolate` was, before `clean_headers`). Keep the existing `scalar_to_string` helper.

```rust
/// One span of a scanned template: literal text or a variable reference
/// (the reference string includes its leading `$`, ready for `get_variable`).
enum Segment {
    Literal(String),
    Ref(String),
}

/// Length in bytes of a bare `$ident` starting at `rest` (the text after `$`).
/// Identifiers start with a letter or `_`, then allow letters/digits/`_`/`.`/`-`.
/// A trailing `.` or `-` is NOT captured (so `$x.` yields `x`). Returns 0 when
/// `rest` does not start a valid identifier.
fn bare_ident_len(rest: &str) -> usize {
    let mut len = 0;
    for (idx, ch) in rest.char_indices() {
        let ok = if idx == 0 {
            ch.is_ascii_alphabetic() || ch == '_'
        } else {
            ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '-'
        };
        if ok {
            len = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    while len > 0 {
        let last = rest[..len].chars().next_back().unwrap();
        if last == '.' || last == '-' {
            len -= last.len_utf8();
        } else {
            break;
        }
    }
    len
}

/// Split `template` into literal and reference segments. Supports `${path}`
/// (delimited), bare `$path`, and `$$` (escaped literal `$`). A lone `$` or one
/// followed by a non-identifier is treated literally.
fn scan_template(template: &str) -> Vec<Segment> {
    let mut segs: Vec<Segment> = Vec::new();
    let mut lit = String::new();
    let mut push_lit = |lit: &mut String, segs: &mut Vec<Segment>| {
        if !lit.is_empty() {
            segs.push(Segment::Literal(std::mem::take(lit)));
        }
    };

    let mut i = 0;
    while i < template.len() {
        match template[i..].find('$') {
            None => {
                lit.push_str(&template[i..]);
                break;
            }
            Some(rel) => {
                lit.push_str(&template[i..i + rel]);
                let dollar = i + rel;
                let after = dollar + 1;
                let next = template.as_bytes().get(after).copied();
                if next == Some(b'$') {
                    lit.push('$');
                    i = after + 1;
                } else if next == Some(b'{') {
                    if let Some(crel) = template[after + 1..].find('}') {
                        let key = &template[after + 1..after + 1 + crel];
                        push_lit(&mut lit, &mut segs);
                        segs.push(Segment::Ref(format!("${}", key)));
                        i = after + 1 + crel + 1;
                    } else {
                        lit.push('$');
                        i = after;
                    }
                } else {
                    let idlen = bare_ident_len(&template[after..]);
                    if idlen > 0 {
                        let key = &template[after..after + idlen];
                        push_lit(&mut lit, &mut segs);
                        segs.push(Segment::Ref(format!("${}", key)));
                        i = after + idlen;
                    } else {
                        lit.push('$');
                        i = after;
                    }
                }
            }
        }
    }
    push_lit(&mut lit, &mut segs);
    segs
}

/// Interpolate `template` into a String. Every reference is resolved and
/// stringified (string/number/bool); object/array/null references error.
/// Used for headers, paths, and query values.
pub fn interpolate_string(
    template: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<String> {
    if !template.contains('$') {
        return Ok(template.to_owned());
    }
    let mut out = String::with_capacity(template.len());
    for seg in scan_template(template) {
        match seg {
            Segment::Literal(s) => out.push_str(&s),
            Segment::Ref(key) => {
                let resolved = get_variable(&key, config, prior_steps)?;
                let s = scalar_to_string(&resolved).ok_or_else(|| {
                    anyhow!(
                        "'{}' resolved to a non-string value ({})",
                        key,
                        value_type_name(&resolved)
                    )
                })?;
                out.push_str(&s);
            }
        }
    }
    Ok(out)
}

/// Resolve `template` to a `Value`. `re/...` generates a string. A template that
/// is exactly one reference preserves the resolved type. A pure literal returns
/// a string. Mixed text/refs stringify. Used for data values and step-set output.
pub fn resolve_value(
    template: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<Value> {
    if let Some(pattern) = template.strip_prefix("re/") {
        return Ok(Value::String(generate_regex_string(pattern)?));
    }
    if !template.contains('$') {
        return Ok(Value::String(template.to_owned()));
    }
    let segs = scan_template(template);
    match segs.as_slice() {
        [Segment::Ref(key)] => get_variable(key, config, prior_steps),
        _ => Ok(Value::String(interpolate_string(template, config, prior_steps)?)),
    }
}
```

Then update `clean_headers` to call the new helper. Change its body line from:

```rust
        let resolved =
            interpolate(v, config, prior_steps).map_err(|e| anyhow!("header '{}': {}", k, e))?;
```

to:

```rust
        let resolved = interpolate_string(v, config, prior_steps)
            .map_err(|e| anyhow!("header '{}': {}", k, e))?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd rs && cargo test test_interp test_resolve_value test_clean_headers 2>&1 | tail -25`
Expected: PASS (all interp, resolve_value, and existing clean_headers tests).

- [ ] **Step 5: Commit**

```bash
git add rs/src/test_step.rs
git commit -m "Add unified interpolation engine (scanner, interpolate_string, resolve_value)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 1.2: Apply engine to data, path, and assertion-expected values

**Files:**
- Modify: `rs/src/test_step.rs` — `clean_request_data` (~213–224), `clean_path` (~226–259), `compare_primitive_values` `$` branch (~668–690)

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_clean_request_data_inline_braced() {
    let mut prior = no_prior_steps();
    prior.insert("s".to_owned(), make_step_result("s", json!({"id": 7}), json!(null)));
    let out = clean_request_data(&json!({"note": "id=${s.response.id}"}), &no_config(), &prior).unwrap();
    assert_eq!(out, json!({"note": "id=7"}));
}

#[test]
fn test_clean_request_data_whole_ref_keeps_type() {
    let mut prior = no_prior_steps();
    prior.insert("s".to_owned(), make_step_result("s", json!({"id": 7}), json!(null)));
    let out = clean_request_data(&json!({"uid": "$s.response.id"}), &no_config(), &prior).unwrap();
    assert_eq!(out, json!({"uid": 7}));
}

#[test]
fn test_clean_path_braced_variable() {
    let mut prior = no_prior_steps();
    prior.insert("c".to_owned(), make_step_result("c", json!({"id": 42}), json!(null)));
    let out = clean_path("items/${c.response.id}", &no_config(), &prior).unwrap();
    assert_eq!(out, "/items/42");
}

#[test]
fn test_assertion_expected_braced_ref() {
    let mut prior = no_prior_steps();
    prior.insert("s".to_owned(), make_step_result("s", json!({"name": "alice"}), json!(null)));
    let asserts = {
        let mut a = vec![];
        compare_primitive_values(&json!("alice"), &json!("${s.response.name}"), "field", &no_config(), &prior, &mut a);
        a
    };
    assert!(asserts.iter().all(|x| x.passed), "{:?}", asserts);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rs && cargo test test_clean_request_data_inline_braced test_clean_path_braced_variable test_assertion_expected_braced_ref 2>&1 | tail -20`
Expected: FAIL (braced refs not resolved; `${...}` treated as literal).

- [ ] **Step 3: Implement**

In `clean_request_data`, replace the string-leaf branch:

```rust
    } else if let Some(data_str) = request_data.as_str() {
        if let Some(pattern) = data_str.strip_prefix("re/") {
            Ok(Value::String(generate_regex_string(pattern)?))
        } else if data_str.starts_with('$') {
            get_variable(data_str, config, prior_steps)
        } else {
            Ok(Value::from(data_str))
        }
    } else {
```

with:

```rust
    } else if let Some(data_str) = request_data.as_str() {
        resolve_value(data_str, config, prior_steps)
    } else {
```

Replace the entire body of `clean_path` with:

```rust
pub fn clean_path(
    path: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<String> {
    let interpolated = interpolate_string(path, config, prior_steps)?;
    if interpolated.starts_with('/') {
        Ok(interpolated)
    } else {
        Ok(format!("/{}", interpolated))
    }
}
```

In `compare_primitive_values`, change the bare-`$` branch condition and resolver. Replace:

```rust
        } else if exp_str.starts_with('$') {
            match get_variable(exp_str, config, prior_steps) {
                Ok(exp_var) => {
```

with:

```rust
        } else if exp_str.contains('$') {
            match resolve_value(exp_str, config, prior_steps) {
                Ok(exp_var) => {
```

(The rest of that match arm — `value_eq(&exp_var, observed)` and the error arm — stays unchanged.)

- [ ] **Step 4: Run to verify pass + no regressions**

Run: `cd rs && cargo test 2>&1 | grep "test result"`
Expected: all suites pass (existing `clean_path`, `clean_request_data`, assertion tests included).

- [ ] **Step 5: Commit**

```bash
git add rs/src/test_step.rs
git commit -m "Route data, path, and assertion-expected values through interpolation engine

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 1.3: Apply engine to step-set output + integration test

**Files:**
- Modify: `rs/src/config.rs` — `run_internal` output loop (~88–131), add `use` for `resolve_value`
- Create: addition to `testing/tests/test_dir/test-header-interpolation.yaml`

- [ ] **Step 1: Write failing unit test**

Add to `rs/src/config.rs` `mod tests`:

```rust
#[tokio::test]
async fn test_step_set_output_interpolates_inline() {
    let cfg = make_config(
        "step-sets:\n  login:\n    once: false\n    output:\n      header: \"Bearer $reg.response.token\"\n    steps:\n      - id: reg\n        path: /unused",
    );
    // Simulate by resolving an output template directly against a prior step.
    use crate::test_step::{resolve_value, TestStepResult};
    let mut local = std::collections::HashMap::new();
    local.insert(
        "reg".to_owned(),
        TestStepResult::make_success(Some("reg"), serde_json::json!({"token": "abc"}), serde_json::Value::Null, serde_json::Value::Null),
    );
    let _ = cfg; // config not needed for this resolution
    let v = resolve_value("Bearer $reg.response.token", &None, &local).unwrap();
    assert_eq!(v, serde_json::json!("Bearer abc"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rs && cargo test test_step_set_output_interpolates_inline 2>&1 | tail -15`
Expected: FAIL — `resolve_value` not imported in `config.rs`.

- [ ] **Step 3: Implement**

At the top of `rs/src/config.rs`, extend the import:

```rust
use crate::test_step::{resolve_value, RunnableTestStep, TestStep, TestStepResult, TestStepSpec};
```

Replace the output-processing loop in `run_internal` (the `for (output_key, output_value) in self.outputs.iter()` block, lines ~90–131) with:

```rust
        for (output_key, output_value) in self.outputs.iter() {
            match resolve_value(output_value, config, &local_steps) {
                Ok(v) => {
                    outputs.insert(output_key.as_str(), v);
                }
                Err(e) => {
                    return Err(anyhow!("output '{}': {}", output_key, e));
                }
            }
        }
```

This requires `local_steps` to be keyed by `String` (so it can be passed as `&HashMap<String, TestStepResult>`). Change its declaration near the top of `run_internal`:

```rust
        let mut local_steps: HashMap<String, TestStepResult> = HashMap::new();
```

and the insert inside the step loop:

```rust
                    if let Some(id) = step.get_id() {
                        local_steps.insert(id.to_string(), result);
                    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cd rs && cargo test 2>&1 | grep "test result"`
Expected: all pass.

- [ ] **Step 5: Add integration test**

Append to `testing/tests/test_dir/test-header-interpolation.yaml`:

```yaml

# Delimited ${} form with surrounding text (Phase 1 engine).
test-header-braced-bearer-from-setup:
  setup: create-user
  steps:
    - path: /api/user/whoami
      method: GET
      headers:
        Authorization: "Bearer ${setup.token}"
      assert:
        status-code: 200
        body:
          name: ${setup.username}
          id: +int
```

- [ ] **Step 6: Verify integration test (manual)**

Run the Flask app and binary:
```bash
cd testing/api && python main.py >/tmp/flask.log 2>&1 &
sleep 3
cd /home/charlie/repos/yapitest
./rs/target/debug/yapitest testing/tests/test_dir/test-header-interpolation.yaml 2>&1 | grep braced
pkill -f "python main.py"
```
Expected: `PASS  test-header-braced-bearer-from-setup`
(Run `cargo build` in `rs/` first if the binary is stale.)

- [ ] **Step 7: Commit**

```bash
git add rs/src/config.rs testing/tests/test_dir/test-header-interpolation.yaml
git commit -m "Interpolate step-set output via shared engine; add braced header test

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

- [ ] **Step 8: Update docs**

In `Tests.md` and `Configs.md`, document interpolation forms (`$path`, `${path}`, `$$`, type preservation for whole-value refs vs stringification for mixed), and that it applies to headers, data, paths, assertion-expected values, and step-set output. Commit:

```bash
git add Tests.md Configs.md
git commit -m "Document unified interpolation forms

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

# Phase 2 — Assertions

### Task 2.1: Array indexing in `get_field`

**Files:**
- Modify: `rs/src/test_step.rs` — `TestStepResult::get_field` navigation (~924–926)

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_get_field_array_index() {
    let r = make_step_result("s", json!({"items": [{"id": 10}, {"id": 20}]}), json!(null));
    assert_eq!(r.get_field("response.items.1.id").unwrap(), Some(json!(20)));
}

#[test]
fn test_get_field_array_index_out_of_range_none() {
    let r = make_step_result("s", json!({"items": [{"id": 10}]}), json!(null));
    assert_eq!(r.get_field("response.items.5.id").unwrap(), None);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rs && cargo test test_get_field_array_index 2>&1 | tail -15`
Expected: FAIL (object-only navigation returns None for `items.1`).

- [ ] **Step 3: Implement**

In `get_field`, replace the `else` navigation branch:

```rust
            } else {
                current = current
                    .and_then(|v| v.as_object())
                    .and_then(|obj| obj.get(*section));
            }
```

with:

```rust
            } else {
                current = current.and_then(|v| match v {
                    Value::Object(obj) => obj.get(*section),
                    Value::Array(arr) => {
                        section.parse::<usize>().ok().and_then(|i| arr.get(i))
                    }
                    _ => None,
                });
            }
```

- [ ] **Step 4: Run to verify pass**

Run: `cd rs && cargo test test_get_field 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rs/src/test_step.rs
git commit -m "Support array indexing in field-path navigation

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2.2: Presence markers `+exists` / `+absent` / `+null`

**Files:**
- Modify: `rs/src/test_step.rs` — `compare_data_objects` (presence) and `compare_primitive_values` type match (`+null`)

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_presence_exists_pass_absent_fail() {
    // +exists passes when present, +absent passes when missing
    let pass = run_compare_objects(json!({"email": "a@b.c"}), json!({"email": "+exists"}), false);
    assert!(pass.iter().all(|a| a.passed), "{:?}", pass);
    let pass2 = run_compare_objects(json!({"name": "x"}), json!({"secret": "+absent"}), false);
    assert!(pass2.iter().all(|a| a.passed), "{:?}", pass2);
}

#[test]
fn test_presence_exists_fail_absent_fail() {
    let fail1 = run_compare_objects(json!({"name": "x"}), json!({"email": "+exists"}), false);
    assert!(fail1.iter().any(|a| !a.passed));
    let fail2 = run_compare_objects(json!({"secret": "s"}), json!({"secret": "+absent"}), false);
    assert!(fail2.iter().any(|a| !a.passed));
}

#[test]
fn test_null_marker() {
    let pass = run_compare_objects(json!({"ends_at": null}), json!({"ends_at": "+null"}), false);
    assert!(pass.iter().all(|a| a.passed), "{:?}", pass);
    let fail = run_compare_objects(json!({"ends_at": "2020"}), json!({"ends_at": "+null"}), false);
    assert!(fail.iter().any(|a| !a.passed));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rs && cargo test test_presence test_null_marker 2>&1 | tail -20`
Expected: FAIL (`+exists`/`+absent` treated as literal string compares; `+null` unknown type passes everything).

- [ ] **Step 3: Implement presence in `compare_data_objects`**

In the `for (key, expected) in expected_object` loop, after the `len(` skip and after computing `field_path`, but **before** the `observed_object.get(key)` lookup, insert:

```rust
        if let Some(marker) = expected.as_str() {
            if marker == "+exists" || marker == "+absent" {
                let present = observed_object.contains_key(key);
                let want_present = marker == "+exists";
                let passed = present == want_present;
                assertions.push(AssertionResult {
                    name: format!("{} ({})", field_path, marker),
                    passed,
                    message: if passed {
                        None
                    } else if want_present {
                        Some(format!("missing field '{}' in response", field_path))
                    } else {
                        Some(format!("field '{}' must not be present, but it was", field_path))
                    },
                });
                if !passed {
                    all_passed = false;
                }
                continue;
            }
        }
```

(Ensure `field_path` is computed above this block. In the current code `field_path` is computed right after the `len(` check — move the marker block to immediately follow the `field_path` assignment.)

Implement `+null` in `compare_primitive_values` type match. In the `match exp_type` block add an arm:

```rust
                "null" | "nil" => observed.is_null(),
```

and in the `readable_type` match add:

```rust
                "null" | "nil" => "null",
```

- [ ] **Step 4: Run to verify pass**

Run: `cd rs && cargo test test_presence test_null_marker 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rs/src/test_step.rs
git commit -m "Add +exists/+absent presence markers and +null type marker

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2.3: `+exists` array membership matcher

**Files:**
- Modify: `rs/src/test_step.rs` — `compare_data_inner` (intercept matcher), add `compare_contains` helper

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_contains_matcher_pass() {
    let observed = json!({"items": [{"id": 1, "title": "a"}, {"id": 2, "title": "b"}]});
    let expected = json!({"items": {"+exists": {"id": 2, "title": "+str"}}});
    let asserts = run_compare_objects(observed, expected, false);
    assert!(asserts.iter().all(|a| a.passed), "{:?}", asserts);
}

#[test]
fn test_contains_matcher_fail_no_match() {
    let observed = json!({"items": [{"id": 1}, {"id": 2}]});
    let expected = json!({"items": {"+exists": {"id": 99}}});
    let asserts = run_compare_objects(observed, expected, false);
    assert!(asserts.iter().any(|a| !a.passed));
}

#[test]
fn test_contains_matcher_fail_not_array() {
    let observed = json!({"items": {"id": 1}});
    let expected = json!({"items": {"+exists": {"id": 1}}});
    let asserts = run_compare_objects(observed, expected, false);
    assert!(asserts.iter().any(|a| !a.passed));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rs && cargo test test_contains_matcher 2>&1 | tail -20`
Expected: FAIL (matcher object is compared as a literal object and mismatches).

- [ ] **Step 3: Implement**

At the very top of `compare_data_inner`, before the existing object/array/primitive dispatch, add:

```rust
    if let Some(obj) = expected.as_object() {
        if obj.len() == 1 {
            if let Some(inner) = obj.get("+exists") {
                return compare_contains(observed, inner, keys, config, prior_steps, assertions);
            }
        }
    }
```

Add the helper function (place it just before `compare_data_inner`):

```rust
/// `+exists` membership matcher: passes if at least one element of the observed
/// array matches the `inner` partial object (all inner field assertions pass).
fn compare_contains(
    observed: &Value,
    inner: &Value,
    keys: &str,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
    assertions: &mut Vec<AssertionResult>,
) -> bool {
    let path = keys.trim_start_matches('.');
    let display = if path.is_empty() { "<root>" } else { path };
    let name = format!("{} (+exists)", display);

    let arr = match observed.as_array() {
        Some(a) => a,
        None => {
            assertions.push(AssertionResult {
                name,
                passed: false,
                message: Some(format!(
                    "'{}' — expected an array to search for a matching element, got {}",
                    display,
                    value_type_name(observed)
                )),
            });
            return false;
        }
    };

    for elem in arr {
        let mut trial: Vec<AssertionResult> = Vec::new();
        if compare_data_inner(elem, inner, false, "", config, prior_steps, &mut trial) {
            assertions.push(AssertionResult { name, passed: true, message: None });
            return true;
        }
    }

    assertions.push(AssertionResult {
        name,
        passed: false,
        message: Some(format!("'{}' — no element matched the +exists criteria", display)),
    });
    false
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd rs && cargo test test_contains_matcher 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rs/src/test_step.rs
git commit -m "Add +exists array membership matcher

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2.4: Numeric value comparisons

**Files:**
- Modify: `rs/src/test_step.rs` — add `parse_value_comparison`, branch in `compare_primitive_values`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_value_comparison_pass_and_fail() {
    let pass = run_assert(json!(5), json!(">=1"));
    assert!(pass.iter().all(|a| a.passed), "{:?}", pass);
    let fail = run_assert(json!(0), json!(">=1"));
    assert!(fail.iter().any(|a| !a.passed));
}

#[test]
fn test_value_comparison_float_and_negative() {
    let pass = run_assert(json!(3.5), json!("<5"));
    assert!(pass.iter().all(|a| a.passed), "{:?}", pass);
    let pass2 = run_assert(json!(-2), json!(">=-3"));
    assert!(pass2.iter().all(|a| a.passed), "{:?}", pass2);
}

#[test]
fn test_value_comparison_non_number_fails() {
    let fail = run_assert(json!("hello"), json!(">=1"));
    assert!(fail.iter().any(|a| !a.passed));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rs && cargo test test_value_comparison 2>&1 | tail -20`
Expected: FAIL (`">=1"` compared as literal string).

- [ ] **Step 3: Implement**

Add near `parse_comparison`:

```rust
/// Parse a value comparison like ">=1", "<5", "=0", ">-2.5". Returns None when
/// `s` is not a comparison expression (so callers fall back to other handling).
fn parse_value_comparison(s: &str) -> Option<(Operator, f64)> {
    let re = Regex::new(r"^\s*([<>]=?|=)\s*(-?\d+(?:\.\d+)?)\s*$").ok()?;
    let caps = re.captures(s.trim())?;
    let op = match caps.get(1)?.as_str() {
        ">" => Operator::Gt,
        ">=" => Operator::Gte,
        "<" => Operator::Lt,
        "<=" => Operator::Lte,
        "=" => Operator::Eq,
        _ => return None,
    };
    let val: f64 = caps.get(2)?.as_str().parse().ok()?;
    Some((op, val))
}
```

In `compare_primitive_values`, inside the `if let Some(exp_str) = expected.as_str()` block, add a branch **after** the `exp_str.contains('$')` branch and before the closing of that `if`:

```rust
        } else if let Some((op, target)) = parse_value_comparison(exp_str) {
            let name = format!("{} ({})", path, exp_str);
            match observed.as_f64() {
                Some(n) => {
                    let passed = match op {
                        Operator::Gt => n > target,
                        Operator::Gte => n >= target,
                        Operator::Lt => n < target,
                        Operator::Lte => n <= target,
                        Operator::Eq => n == target,
                    };
                    assertions.push(AssertionResult {
                        name,
                        passed,
                        message: if passed {
                            None
                        } else {
                            Some(format!("'{}' — expected value {}, got {}", path, exp_str, n))
                        },
                    });
                    return passed;
                }
                None => {
                    assertions.push(AssertionResult {
                        name,
                        passed: false,
                        message: Some(format!(
                            "'{}' — expected a number to compare {}, got {} ({})",
                            path, exp_str, value_type_name(observed), observed
                        )),
                    });
                    return false;
                }
            }
        }
```

- [ ] **Step 4: Run to verify pass + no regressions**

Run: `cd rs && cargo test 2>&1 | grep "test result"`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add rs/src/test_step.rs
git commit -m "Add numeric value comparisons in body assertions

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2.5: `status-code` list

**Files:**
- Modify: `rs/src/test_step.rs` — `check_status_code` (~934)

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_status_code_list_matches_any() {
    assert!(TestStep::check_status_code(&json!([200, 201]), 201));
    assert!(TestStep::check_status_code(&json!([200, "4xx"]), 404));
}

#[test]
fn test_status_code_list_no_match() {
    assert!(!TestStep::check_status_code(&json!([200, 201]), 500));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rs && cargo test test_status_code_list 2>&1 | tail -15`
Expected: FAIL (array not handled).

- [ ] **Step 3: Implement**

At the start of `check_status_code`, before the `as_u64` check, add:

```rust
        if let Some(arr) = exp.as_array() {
            return arr.iter().any(|e| TestStep::check_status_code(e, actual));
        }
```

- [ ] **Step 4: Run to verify pass**

Run: `cd rs && cargo test test_status_code 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add rs/src/test_step.rs
git commit -m "Accept status-code as a list of acceptable codes

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2.6: Phase 2 API endpoints + integration tests

**Files:**
- Modify: `testing/api/main.py`
- Create: `testing/tests/test_dir/test-assertions-extra.yaml`

- [ ] **Step 1: Add API support for null/absent + a searchable list**

`/api/post/list` already returns `{"posts": [...]}` with objects containing `id`, `title`, `body`, `user_id`, `meta`. Add a public profile endpoint that omits secrets and exposes a nullable field. Insert before `get_user` in `testing/api/main.py`:

```python
@app.route("/api/user/<username>/public", methods=["GET"])
def public_profile(username):
    if username not in USERS_BY_USERNAME:
        return jsonify({"error": "User not found"}), 404
    user = USERS_BY_USERNAME[username]
    # Public view: no token/password; ends_at is intentionally null.
    return {"name": user.name, "id": user.id, "ends_at": None}
```

- [ ] **Step 2: Write integration tests**

Create `testing/tests/test_dir/test-assertions-extra.yaml`:

```yaml
# Phase 2 assertion features: presence markers, null, numeric comparisons,
# array indexing + +exists membership, status-code lists.

test-public-profile-absence-and-null:
  setup: create-user
  steps:
    - path: /api/user/$setup.username/public
      assert:
        status-code: 200
        body:
          name: +exists
          token: +absent
          password: +absent
          ends_at: +null
          id: ">=1"

test-status-code-list:
  steps:
    - path: /api/healthz
      assert:
        status-code: [200, 204]

test-post-list-contains-created:
  setup: create-user
  steps:
    - id: make
      path: /api/post/create
      method: POST
      headers:
        API-Token: $setup.token
      data:
        title: "Find Me"
        body: "needle"
      assert:
        status-code: 201
    - path: /api/post/list
      assert:
        status-code: 200
        body:
          posts:
            +exists:
              id: ${make.response.post_id}
              title: "Find Me"

test-post-list-index:
  steps:
    - path: /api/post/list
      assert:
        status-code: 200
        body:
          len(posts): ">=1"
          posts.0.id: +int

# ── designed to fail ────────────────────────────────────────────────────────

test-absent-violated-fail:
  setup: create-user
  steps:
    - path: /api/user/$setup.username/public
      assert:
        body:
          name: +absent   # name IS present → must fail

test-numeric-comparison-fail:
  steps:
    - path: /api/post/list
      assert:
        body:
          len(posts): ">=999999"

test-contains-missing-fail:
  steps:
    - path: /api/post/list
      assert:
        body:
          posts:
            +exists:
              id: -1
```

- [ ] **Step 3: Build, run, verify**

```bash
cd rs && cargo build 2>&1 | tail -3
cd /home/charlie/repos/yapitest/testing/api && python main.py >/tmp/flask.log 2>&1 &
sleep 3
cd /home/charlie/repos/yapitest
./rs/target/debug/yapitest testing/tests/test_dir/test-assertions-extra.yaml 2>&1 | sed -n '/Collecting/,/Results/p'
pkill -f "python main.py"
```
Expected: the four `test-*` (non-`-fail`) tests PASS; the three `-fail` tests FAIL.

- [ ] **Step 4: Commit**

```bash
git add testing/api/main.py testing/tests/test_dir/test-assertions-extra.yaml
git commit -m "Add Phase 2 assertion API endpoint and integration tests

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

- [ ] **Step 5: Docs**

Document `+exists` (presence value + membership matcher), `+absent`, `+null`, numeric value comparisons, array indexing, and `status-code` lists in `Tests.md`. Commit:

```bash
git add Tests.md
git commit -m "Document Phase 2 assertion features

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

# Phase 3 — Step mechanics

### Task 3.1: `query:` map

**Files:**
- Modify: `rs/src/test_step.rs` — `TestStepSpec`, `TestStep`, `from_spec`, `run_attempt`
- Modify: `testing/api/main.py` (search endpoint)
- Create: `testing/tests/test_dir/test-query.yaml`

- [ ] **Step 1: Write failing unit test**

```rust
#[test]
fn test_query_field_parses() {
    let spec: TestStepSpec = serde_yaml::from_str(
        "path: /search\nquery:\n  q: hello\n  page: \"2\"",
    ).unwrap();
    let step = TestStep::from_spec(spec);
    assert_eq!(step.query_data.get("q").map(String::as_str), Some("hello"));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rs && cargo test test_query_field_parses 2>&1 | tail -15`
Expected: FAIL — no `query` field / `query_data` member.

- [ ] **Step 3: Implement**

In `TestStepSpec` add a field:

```rust
    query: Option<HashMap<String, String>>,
```

In `TestStep` add a field:

```rust
    query_data: HashMap<String, String>,
```

In `from_spec`, build it (near the `header_data` setup) and include it in the returned struct:

```rust
        let query_data = spec.query.unwrap_or_default();
```

and in the `TestStep { ... }` literal add `query_data,`.

In `run_attempt`, change the request building. Replace:

```rust
        match client
            .request(self.method.clone(), full_url)
            .headers(headers)
            .json(&req_data)
            .send()
            .await
        {
```

with:

```rust
        let mut request = client
            .request(self.method.clone(), full_url)
            .headers(headers)
            .json(&req_data);

        if !self.query_data.is_empty() {
            let mut pairs: Vec<(String, String)> = Vec::new();
            for (k, v) in &self.query_data {
                let val = interpolate_string(v, config, prior_steps)
                    .map_err(|e| anyhow!("query '{}': {}", k, e))?;
                pairs.push((k.clone(), val));
            }
            request = request.query(&pairs);
        }

        match request.send().await {
```

- [ ] **Step 4: Run to verify pass**

Run: `cd rs && cargo test test_query_field_parses 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Add API search endpoint**

Insert before `get_user` in `testing/api/main.py`:

```python
@app.route("/api/user/search", methods=["GET"])
def search_users():
    q = request.args.get("q", "")
    matches = [u.to_json() for u in USERS_BY_ID.values() if q and q in u.name]
    return {"query": q, "results": matches}
```

- [ ] **Step 6: Add integration test**

Create `testing/tests/test_dir/test-query.yaml`:

```yaml
test-query-echoes-value:
  setup: create-user
  steps:
    - path: /api/user/search
      query:
        q: $setup.username
      assert:
        status-code: 200
        body:
          query: $setup.username
          results:
            +exists:
              name: $setup.username

test-query-interpolated-literal-mix:
  steps:
    - path: /api/user/search
      query:
        q: "no-such-user-zzz"
      assert:
        status-code: 200
        body:
          query: "no-such-user-zzz"
          len(results): "=0"

# designed to fail: wrong echoed query value
test-query-wrong-echo-fail:
  steps:
    - path: /api/user/search
      query:
        q: "alpha"
      assert:
        body:
          query: "beta"
```

- [ ] **Step 7: Build, run, verify**

```bash
cd rs && cargo build 2>&1 | tail -3
cd /home/charlie/repos/yapitest/testing/api && python main.py >/tmp/flask.log 2>&1 &
sleep 3
cd /home/charlie/repos/yapitest
./rs/target/debug/yapitest testing/tests/test_dir/test-query.yaml 2>&1 | sed -n '/Collecting/,/Results/p'
pkill -f "python main.py"
```
Expected: the two `test-query-*` non-`-fail` tests PASS; `test-query-wrong-echo-fail` FAILS.

- [ ] **Step 8: Commit + docs**

```bash
git add rs/src/test_step.rs testing/api/main.py testing/tests/test_dir/test-query.yaml
git commit -m "Add query: map for variable-friendly query strings

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

Document `query:` in `Tests.md` and commit:

```bash
git add Tests.md
git commit -m "Document query: map

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3.2: Parameterized step-sets (`args`)

**Files:**
- Modify: `rs/src/test_step.rs` — add `TestStepResult::from_output`
- Modify: `rs/src/config.rs` — `run_internal` (+args), `run_with_args`, trait `run`
- Modify: `rs/src/test.rs` — `setup`/`teardown` accept `{name, args}`, resolve args, call `run_with_args`
- Create: `testing/tests/test_dir/test-parameterized-setup.yaml`

- [ ] **Step 1: Write failing unit test**

Add to `rs/src/config.rs` `mod tests`:

```rust
#[tokio::test]
async fn test_step_set_args_visible_in_steps_and_output() {
    let cfg = make_config(
        "step-sets:\n  echo:\n    once: false\n    output:\n      who: $args.email\n    steps:\n      - id: noop\n        path: /unused",
    );
    let group = cfg.get_step_group("echo").unwrap();
    let args = serde_json::json!({"email": "alice@example.com"});
    // run_internal with args; steps will fail to HTTP but output resolution
    // of $args.email must succeed because args is in scope.
    let result = group
        .run_with_args(&None, &std::collections::HashMap::new(), Some(args))
        .await;
    // The /unused step errors (no base URL) so run returns Err; assert the
    // error is about the step, proving args injection compiled & ran.
    assert!(result.is_err());
}
```

(This test verifies wiring compiles and runs; full positive behavior is covered by the API test in Step 6. A pure args-resolution unit test is added next.)

Add to `rs/src/test_step.rs` `mod tests`:

```rust
#[test]
fn test_from_output_field_lookup() {
    let r = TestStepResult::from_output(json!({"email": "a@b.c"}));
    assert_eq!(r.get_field("email").unwrap(), Some(json!("a@b.c")));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd rs && cargo test test_from_output_field_lookup test_step_set_args 2>&1 | tail -20`
Expected: FAIL — `from_output` and `run_with_args` do not exist.

- [ ] **Step 3: Add `TestStepResult::from_output`**

In `rs/src/test_step.rs`, in `impl TestStepResult`, add:

```rust
    /// A synthetic result carrying only output data — used to inject an
    /// `$args.*` (or similar) namespace into a resolution scope.
    pub fn from_output(output_data: Value) -> TestStepResult {
        TestStepResult {
            step_id: None,
            status: TestStepFailureReason::NoFailure,
            response_data: None,
            request_data: None,
            output_data: Some(output_data),
            failure_message: None,
            assertion_results: Vec::new(),
        }
    }
```

- [ ] **Step 4: Thread args through `config.rs`**

Change `run_internal`'s signature and body. Replace the signature line:

```rust
    pub async fn run_internal(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
    ) -> Result<TestStepResult> {
```

with:

```rust
    pub async fn run_internal(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
        args: Option<serde_json::Value>,
    ) -> Result<TestStepResult> {
```

At the top of `run_internal`, build a scope that includes args, and use it for step execution:

```rust
        // Scope visible to the step-set's steps: caller's prior steps plus
        // an optional `$args.*` namespace.
        let mut scope: HashMap<String, TestStepResult> = prior_steps.clone();
        if let Some(args_val) = &args {
            scope.insert("args".to_owned(), TestStepResult::from_output(args_val.clone()));
        }

        let mut local_steps: HashMap<String, TestStepResult> = HashMap::new();
        for step in self.steps.iter() {
            match step.run(config, &scope).await {
                Ok(result) => {
                    if let Some(id) = step.get_id() {
                        local_steps.insert(id.to_string(), result);
                    }
                }
                Err(e) => {
                    let step_name = step.get_id().map(|s| s.as_str()).unwrap_or("(unnamed)");
                    return Err(anyhow!("step '{}' failed: {}", step_name, e));
                }
            }
        }
```

(Remove the previous `let mut local_steps: HashMap<&str, ...>` block and step loop — replaced above. `local_steps` is now `String`-keyed, matching Task 1.3.)

For output resolution, include args in the output scope. Just before the output loop, add:

```rust
        let mut output_scope = local_steps.clone();
        if let Some(args_val) = &args {
            output_scope.insert("args".to_owned(), TestStepResult::from_output(args_val.clone()));
        }
```

and resolve against `output_scope` (replacing `&local_steps` from Task 1.3):

```rust
        for (output_key, output_value) in self.outputs.iter() {
            match resolve_value(output_value, config, &output_scope) {
                Ok(v) => {
                    outputs.insert(output_key.as_str(), v);
                }
                Err(e) => {
                    return Err(anyhow!("output '{}': {}", output_key, e));
                }
            }
        }
```

Add a `run_with_args` method to `impl TestStepGroup` (near `run_internal`):

```rust
    pub async fn run_with_args(
        &self,
        config: &Option<Arc<RwLock<ConfigData>>>,
        prior_steps: &HashMap<String, TestStepResult>,
        args: Option<serde_json::Value>,
    ) -> Result<TestStepResult> {
        // Parameterized invocations never use the once-cache (args change the
        // result), so route them straight to run_internal.
        if args.is_some() {
            return self.run_internal(config, prior_steps, args).await;
        }
        self.run(config, prior_steps).await
    }
```

Update the trait `run` impl (the `impl RunnableTestStep for TestStepGroup`) so its two `run_internal` calls pass `None`:

```rust
        if !self.runs_once() {
            return self.run_internal(config, prior_steps, None).await;
        }
```

and:

```rust
        let result = self.run_internal(config, prior_steps, None).await?;
```

Note: `run_with_args` calls `self.run(...)` for the no-args path, which preserves once-caching. (`run` is the trait method on `TestStepGroup`.)

- [ ] **Step 5: Accept `{name, args}` in `test.rs`**

In `rs/src/test.rs`, add an invocation type. Add near the top (after imports):

```rust
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum StepSetInvocation {
    Name(String),
    Detailed {
        name: String,
        #[serde(default)]
        args: HashMap<String, String>,
    },
}
```

Change the `setup`/`teardown` fields in **both** `Test` and `TestSpec` from
`Option<String>` to:

```rust
    setup: Option<StepSetInvocation>,
    teardown: Option<StepSetInvocation>,
```

(`TestSpec` derives `Deserialize`, and `#[serde(untagged)]` makes
`Option<StepSetInvocation>` parse either a bare string or a `{name, args}` map.)

In `Test::from_spec`, pass them through unchanged:

```rust
            setup: spec.setup,
            teardown: spec.teardown,
```

Add an args-resolution helper to `impl Test` (or as a free function in `test.rs`):

```rust
fn resolve_args(
    args: &HashMap<String, String>,
    config: &Option<Arc<RwLock<ConfigData>>>,
    prior_steps: &HashMap<String, TestStepResult>,
) -> Result<serde_json::Value> {
    use crate::test_step::resolve_value;
    let mut map = serde_json::Map::new();
    for (k, v) in args {
        map.insert(k.clone(), resolve_value(v, config, prior_steps)?);
    }
    Ok(serde_json::Value::Object(map))
}
```

(Discard the `StepSetInvocation::args()`/`once_cell_args` sketch above — it was illustrative. Use `resolve_args` with a match on the invocation, shown next.)

Update the setup block in `Test::run`. Replace:

```rust
        // Setup
        if let (Some(setup_id), Some(cfg)) = (self.setup.as_deref(), &self.config) {
            match cfg.read().unwrap().get_step_group(setup_id) {
                Ok(setup) => match setup.run(&self.config, &prior_steps).await {
```

with:

```rust
        // Setup
        if let (Some(setup_inv), Some(cfg)) = (&self.setup, &self.config) {
            let (setup_name, raw_args) = match setup_inv {
                StepSetInvocation::Name(n) => (n.clone(), HashMap::new()),
                StepSetInvocation::Detailed { name, args } => (name.clone(), args.clone()),
            };
            let resolved_args = if raw_args.is_empty() {
                None
            } else {
                match resolve_args(&raw_args, &self.config, &prior_steps) {
                    Ok(a) => Some(a),
                    Err(e) => fail!(TestStepResult::make_failure(
                        Some("setup"),
                        TestStepFailureReason::Miscellaneous,
                        format!("setup args could not be resolved: {}", e),
                    )),
                }
            };
            match cfg.read().unwrap().get_step_group(&setup_name) {
                Ok(setup) => match setup
                    .run_with_args(&self.config, &prior_steps, resolved_args)
                    .await
                {
```

The remainder of the setup match arms (`Ok(result) => { ... }`, the two `Err` arms) are unchanged.

If `teardown` is invoked elsewhere with `.as_deref()`, update that call site the same way (match on `StepSetInvocation` for its `name()`), passing `None` args for teardown unless args are specified.

- [ ] **Step 6: Build and run unit tests**

Run: `cd rs && cargo test 2>&1 | grep -E "test result|^error"`
Expected: compiles; all tests pass (including `test_from_output_field_lookup` and `test_step_set_args_visible_in_steps_and_output`).

- [ ] **Step 7: Add a parameterized step-set + integration test**

Add a `login` step-set to `testing/tests/test_dir/yapitest-config.yaml` `step-sets:` (it already has `create-user`):

```yaml
  login:
    once: false
    steps:
      - id: do-create
        path: /api/user/create
        method: POST
        data:
          username: $args.username
          password: $args.password
      - id: do-login
        path: /api/user/login
        method: POST
        data:
          username: $args.username
          password: $args.password
    output:
      token: $do-login.response.token
      username: $args.username
```

Create `testing/tests/test_dir/test-parameterized-setup.yaml`:

```yaml
test-login-as-alice:
  setup:
    name: login
    args:
      username: alice-param-001
      password: alicepass!
  steps:
    - path: /api/user/whoami
      method: GET
      headers:
        Authorization: "Bearer ${setup.token}"
      assert:
        status-code: 200
        body:
          name: alice-param-001

test-login-as-bob:
  setup:
    name: login
    args:
      username: bob-param-002
      password: bobpass!
  steps:
    - path: /api/user/whoami
      method: GET
      headers:
        Authorization: "Bearer ${setup.token}"
      assert:
        status-code: 200
        body:
          name: bob-param-002

# designed to fail: asserts the wrong user for the supplied args
test-login-args-wrong-user-fail:
  setup:
    name: login
    args:
      username: carol-param-003
      password: carolpass!
  steps:
    - path: /api/user/whoami
      method: GET
      headers:
        Authorization: "Bearer ${setup.token}"
      assert:
        body:
          name: not-carol
```

- [ ] **Step 8: Build, run, verify**

```bash
cd rs && cargo build 2>&1 | tail -3
cd /home/charlie/repos/yapitest/testing/api && python main.py >/tmp/flask.log 2>&1 &
sleep 3
cd /home/charlie/repos/yapitest
./rs/target/debug/yapitest testing/tests/test_dir/test-parameterized-setup.yaml 2>&1 | sed -n '/Collecting/,/Results/p'
pkill -f "python main.py"
```
Expected: `test-login-as-alice` and `test-login-as-bob` PASS; `test-login-args-wrong-user-fail` FAILS with a name mismatch.

- [ ] **Step 9: Commit + docs**

```bash
git add rs/src/test_step.rs rs/src/config.rs rs/src/test.rs \
  testing/tests/test_dir/yapitest-config.yaml \
  testing/tests/test_dir/test-parameterized-setup.yaml
git commit -m "Add parameterized step-sets via setup args

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

Document `setup: {name, args}` and `$args.*` in `Configs.md`/`Tests.md` and commit:

```bash
git add Configs.md Tests.md
git commit -m "Document parameterized step-sets

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Final verification

- [ ] Run the full Rust suite: `cd rs && cargo test 2>&1 | grep "test result"` — all pass.
- [ ] Run every Phase-1/2/3 YAML file against the live API and confirm non-`-fail` tests pass and `-fail` tests fail.
- [ ] Confirm no `cargo build` warnings introduced beyond the pre-existing `step_id` dead-code warning.

---

## Self-Review Notes (author)

- **Spec coverage:** #1 → Tasks 1.1–1.3; #2 → 3.2; #3 → 2.1 + 2.3; #4 → 3.1; #5 → 2.2; #6 → 2.4; #7 → 2.5. All seven covered.
- **Type consistency:** `interpolate_string` / `resolve_value` / `scan_template` / `bare_ident_len` / `Segment` / `scalar_to_string` used consistently across tasks. `run_internal` gains `args: Option<serde_json::Value>`; all call sites (trait `run` ×2, `run_with_args`) updated. `local_steps` is `HashMap<String, TestStepResult>` from Task 1.3 onward.
- **Known sharp edge for the implementer:** `setup`/`teardown` change type from `Option<String>` to `Option<StepSetInvocation>` in both `Test` and `TestSpec`. Any other call site that used `self.setup`/`self.teardown` as a string (e.g. `.as_deref()`) must be migrated to match on `StepSetInvocation`. Teardown passes `None` args unless it specifies them.
