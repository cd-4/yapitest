# Duration Assertion Design Spec

**Date:** 2026-04-14
**Feature:** `duration` assertion in test step `assert` block
**Scope:** Rust implementation only (`rs/src/test_step.rs`)

---

## Goal

Allow test steps to assert that the full round-trip HTTP request (send + receive body) completed in less than a specified duration. If the assertion fails, it is reported alongside other assertion results — it never causes early exit or skips other assertions.

---

## YAML Surface

`duration` is an optional field under the `assert` block:

```yaml
assert:
  status-code: 200
  duration: 500ms
  body:
    token: +str
```

### Accepted formats

| YAML value | Interpreted as |
|---|---|
| `500` | 500 milliseconds (bare YAML integer) |
| `"500"` | 500 milliseconds (bare string integer) |
| `"500ms"` | 500 milliseconds |
| `"2s"` | 2 seconds (= 2000 ms) |

Any other value (e.g. `"fast"`, `"2m"`, `"1.5s"`) is a configuration error reported before any requests fire.

### Assertion semantics

- Passes if `elapsed < limit`
- Fails if `elapsed >= limit`
- Assertion name in output: `"duration"`
- Failure message: `"request took Nms, expected less than Nms"`

---

## Implementation

All changes are in `rs/src/test_step.rs`.

### 1. `TestStepAssertionSpec`

Add one field:

```rust
duration: Option<Value>,
```

Using `Option<Value>` (same as `status_code`) so both bare YAML integers and strings deserialize correctly.

### 2. `parse_duration()` free function

```rust
fn parse_duration(v: &Value) -> Result<std::time::Duration>
```

Logic:
- `Value::Number(n)` → `n.as_u64()` milliseconds; if the number is a float (e.g. `1.5`) or negative, return `Err`
- `Value::String(s)`:
  - strip suffix `"ms"` → parse remainder as `u64` milliseconds
  - strip suffix `"s"` → parse remainder as `u64` seconds
  - otherwise try parsing whole string as `u64` milliseconds
- Any parse failure → `Err(anyhow!("invalid duration '{}' — use '500ms', '2s', or a bare integer (milliseconds)", s))`

### 3. `TestStep` struct

Add one field:

```rust
expected_duration: Option<std::time::Duration>,
```

### 4. `TestStep::from_spec()`

After parsing other assert fields, call `parse_duration()` on the duration value if present:

```rust
if let Some(dur_val) = assertion_data.duration {
    expected_duration = Some(parse_duration(&dur_val)?);
}
```

`from_spec` currently returns `TestStep` (infallible). It must change to `Result<TestStep>` to propagate parse errors. Callers must be updated accordingly.

### 5. `TestStep::run()`

Wrap the HTTP send + body read in a `std::time::Instant`:

```rust
let t0 = std::time::Instant::now();
let response = client.request(...).send().await?;
let res_text = response.text().await?;
let elapsed = t0.elapsed();
```

After all other assertions complete, if `expected_duration` is set:

```rust
if let Some(limit) = self.expected_duration {
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
                limit.as_millis()
            ))
        },
    });
}
```

This is a normal assertion — no early return, no effect on other assertions.

---

## Error Handling

| Scenario | Behavior |
|---|---|
| Valid format (`500ms`, `2s`, `500`) | Parsed to `Duration` at load time |
| Invalid string (`"fast"`, `"2m"`) | `parse_duration` returns `Err`; surfaced as `ConfigurationError` before any HTTP calls |
| Missing `duration` field | `None` — no timing assertion performed |

---

## `from_spec` Signature Change

`from_spec` currently returns `TestStep` (infallible). Adding duration parsing requires it to return `Result<TestStep>`. This is a small but necessary change — call sites in `test.rs` or `main.rs` must propagate the error.

---

## Testing

Unit tests appended to `test_step.rs` in a `#[cfg(test)]` module:

| Test | Description |
|---|---|
| `test_parse_duration_bare_int_value` | `Value::Number(500)` → 500ms |
| `test_parse_duration_bare_int_string` | `"500"` → 500ms |
| `test_parse_duration_ms_suffix` | `"250ms"` → 250ms |
| `test_parse_duration_s_suffix` | `"2s"` → 2000ms |
| `test_parse_duration_invalid` | `"fast"` → `Err` with message containing `"invalid duration"` |
| `test_parse_duration_float_rejected` | `"1.5s"` → `Err` |
