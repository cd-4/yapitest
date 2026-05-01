# Waits and Retries — Design

**Date:** 2026-04-30
**Branch:** waits-and-retries

---

## Overview

Add three new optional fields to test step objects: `wait-before`, `wait-after`, and `retry`. These give users control over timing and resilience without requiring external orchestration.

---

## YAML Interface

```yaml
- id: my-step
  path: /api/endpoint
  wait-before: 500       # sleep before first attempt (ms, "500ms", "2s")
  wait-after: 1s         # sleep after final result
  retry: 3               # retry up to 3 times on assertion failure
  assert:
    status-code: 200
```

### `wait-before` *(optional)*

Duration to sleep before the step's first HTTP attempt. Same format as `assert.duration`: bare integer = milliseconds, or a string with `ms` or `s` suffix (e.g. `500`, `"500ms"`, `"2s"`). Does not repeat between retry attempts. Default: absent (no sleep).

### `wait-after` *(optional)*

Duration to sleep after the step's final result — whether that result is a pass or a fail. Same format as `wait-before`. Default: absent (no sleep).

### `retry` *(optional)*

Non-negative integer. On any assertion failure (status code mismatch, body assertion failure, JSON decode error), the step reruns its HTTP call and assertions up to `retry` additional times. `wait-before` and `wait-after` do **not** repeat between attempts. Default: `0` (no retries).

---

## Implementation

### Structs (`test_step.rs`)

**`TestStepSpec`** — three new deserialized fields:
```rust
wait_before: Option<Value>,
wait_after: Option<Value>,
retry: Option<u32>,
```

**`TestStep`** — three new runtime fields:
```rust
wait_before: Option<Value>,
wait_after: Option<Value>,
retry: u32,
```

**`TestStep::from_spec()`** — copy through directly; `retry` defaults to `0`:
```rust
wait_before: spec.wait_before,
wait_after: spec.wait_after,
retry: spec.retry.unwrap_or(0),
```

### Execution (`test_step.rs`)

Extract the current HTTP + assertion body of `run()` into a private `async fn run_attempt(...)` on `TestStep`. The method signature is identical to `run()` and its body is the current `run()` body unchanged.

`RunnableTestStep::run()` becomes:

```
1. If wait_before is set: parse duration, sleep, return ConfigurationError if invalid
2. For attempt in 0..=self.retry:
     result = self.run_attempt(config, prior_steps).await
     break if result is Ok(NoFailure) or Err (hard error)
3. If wait_after is set: parse duration, sleep, return ConfigurationError if invalid
4. Return last result
```

Hard errors (`Err`) from `run_attempt` (e.g. network failure) break the retry loop immediately and propagate. Only `Ok(failure)` results (assertion failures) trigger retries.

### Tests (`test_step.rs`)

New unit tests in `#[cfg(test)]`:
- `TestStepSpec` deserializes `wait-before`, `wait-after`, `retry` from YAML
- Missing fields produce `None`/absent (not an error)
- `retry` field defaults to `0` when absent from spec

Full round-trip (HTTP + retry) testing requires a live server and is handled by the integration test suite in `testing/`.

### Documentation

- `Tests.md` — new `wait-before`, `wait-after`, `retry` entries under **Step fields**
- `docs/tests.md` — same additions mirrored

---

## Non-goals

- No per-retry delay field (can be added later if needed)
- No retry on hard network errors (only assertion failures)
- `wait-before`/`wait-after` do not repeat between retry attempts
