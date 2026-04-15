# Regex Generate & Match Design

**Date:** 2026-04-14  
**Branch:** regex-generate  
**Scope:** Rust implementation (`rs/src/`)

## Summary

Add `re/<pattern>` syntax to the yapitest YAML format. In `data` blocks the pattern generates a matching string at request time. In `assert.body` blocks the pattern validates the observed response value against the regex. Both contexts use the same `re/` prefix.

## Syntax

```yaml
# Generate a string in request data
data:
  username: re/[a-z]{8}
  code: re/[A-Z0-9]{6}

# Match a string in assertion body
assert:
  body:
    token: re/[a-zA-Z0-9]{32}
    status: re/(active|pending)
```

## Architecture

### Generation (`clean_request_data` in `test_step.rs`)

Before the existing `$` variable check, add a `re/` prefix check on string values:

1. Strip `re/` prefix to get the raw pattern.
2. Call `regex_generate` to produce a string matching the pattern.
3. Return `Value::String(generated)` in place of the literal.
4. If the pattern is invalid, return `Err` with a descriptive message so the step fails cleanly.

The generated value is embedded in the `Value` returned by `clean_request_data`, which becomes `req_data` stored as `request_data` in `TestStepResult`. This makes generated values accessible via `$step-id.data.field` in subsequent steps.

### Assertion matching (`compare_primitive_values` in `test_step.rs`)

Add a new branch after the `+type` check and before the `$variable` check:

1. If the expected string starts with `re/`, strip the prefix.
2. Compile the regex. If invalid, push a failed `AssertionResult` with the error message and return `false`.
3. Assert the observed value is a string. If not, push a failed assertion with a type error.
4. Test `regex.is_match(observed_str)`. Push pass/fail `AssertionResult` accordingly.
5. Assertion name format: `field_path (re/pattern)`.

### No new modules

Both changes are additions within `test_step.rs`. No new files or modules are needed.

## Data Flow

```
YAML parse
  └─ clean_request_data
       └─ "re/[a-z]+" detected
            └─ regex_generate → "kqjmwbtz"
                 └─ stored in req_data (request_data in TestStepResult)
                      └─ accessible as $step-id.data.field in later steps

assert.body comparison
  └─ compare_primitive_values
       └─ expected "re/[a-z]+" detected
            └─ regex.is_match(observed_str)
                 └─ AssertionResult { passed: true/false }
```

## Error Handling

| Scenario | Behaviour |
|---|---|
| Invalid pattern in `data` | Step fails with `Err("invalid regex pattern 're/...': ...")` |
| Invalid pattern in `assert.body` | Assertion fails with message `"'field' — invalid regex pattern 're/...': ..."` |
| Observed value is not a string (assert) | Assertion fails with `"'field' — expected a string to match re/..., got <Type>"` |
| Observed string does not match | Assertion fails with `"'field' — expected to match re/..., got 'actual'"` |

## Tests

Unit tests in `#[cfg(test)]` module within `test_step.rs`:

1. **Generation produces a match** — `clean_request_data` with `re/[a-z]+` returns a non-empty string that itself matches the pattern.
2. **Generated value is not the literal** — returned value is not the string `"re/[a-z]+"`.
3. **Assertion passes on match** — `compare_primitive_values` with observed `"hello"` and expected `re/[a-z]+` produces a passed assertion.
4. **Assertion fails on no match** — observed `"HELLO"` against `re/[a-z]+` produces a failed assertion.
5. **Assertion fails for non-string observed** — observed `42` (number) against `re/[a-z]+` produces a failed assertion with a type error message.
6. **Bad pattern in data** — `clean_request_data` with `re/[invalid` returns `Err`.
7. **Bad pattern in assert** — `compare_primitive_values` with expected `re/[invalid` produces a failed assertion.
