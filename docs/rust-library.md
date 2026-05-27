# Rust Library

Yapitest can be used as a library inside other Rust projects. Instead of running tests from the command line, you load and run them programmatically and inspect the results in code.

---

## Adding the dependency

```toml
[dependencies]
yapitest = "1.0"
tokio = { version = "1", features = ["full"] }
```

---

## Entry points

There are three ways to run tests depending on how your test data is sourced.

### From a path

Point at a file or directory and get back a `Vec<TestResult>`.

**Async** (requires a tokio runtime):

```rust
use yapitest::run_path;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let results = run_path(Path::new("./tests")).await?;

    for r in &results {
        println!("{}: {}", r.name(), if r.passed() { "PASS" } else { "FAIL" });
    }

    Ok(())
}
```

**Blocking** (no async runtime needed):

```rust
use yapitest::run_path_blocking;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let results = run_path_blocking(Path::new("./tests"))?;
    println!("{} tests ran", results.len());
    Ok(())
}
```

Both functions search the given path recursively for test files (filenames starting or ending with `test`, `.yaml`/`.yml`), apply any config files found in parent directories, and return results with no console output.

---

### From YAML values

If you already have YAML loaded as `serde_yaml::Value` — for example, test definitions embedded in your binary or fetched from a remote source — use `run_from_yaml`:

```rust
use yapitest::run_from_yaml;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let tests_yaml: serde_yaml::Value = serde_yaml::from_str(r#"
        test-create-user:
          steps:
            - path: /api/users
              method: POST
              data:
                name: Alice
              assert:
                status-code: 201
                body:
                  name: Alice
    "#)?;

    let config_yaml: serde_yaml::Value = serde_yaml::from_str(r#"
        urls:
          base: http://localhost:8080
    "#)?;

    let results = run_from_yaml(tests_yaml, Some(config_yaml)).await?;

    for r in &results {
        if !r.passed() {
            eprintln!("FAIL {}: {}", r.name(), r.get_failure_message().unwrap_or(""));
        }
    }

    Ok(())
}
```

The second argument is an optional config. Pass `None` if the config is embedded in the test YAML under a `config:` key, or if the tests reference a base URL set by an environment variable.

---

### Lower-level: load then run

For filtering, custom test selection, or running subsets, use `load_tests` and `run_tests` directly:

```rust
use yapitest::{load_tests, run_tests};
use std::collections::HashMap;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::fs::canonicalize("./tests")?;
    let mut configs = HashMap::new();
    let mut tests = load_tests(&mut configs, &path)?;

    // Filter to only tests whose name contains "user"
    tests.retain(|t| t.name.contains("user"));

    // Run with 4 threads, no console output (verbosity 0)
    let results = run_tests(&tests, Some(4), 0).await;

    let failures: Vec<_> = results.iter().filter(|r| !r.passed()).collect();
    println!("{} failed", failures.len());

    Ok(())
}
```

`load_tests` takes a mutable config map (used to cache and chain configs found during directory traversal) and returns a flat `Vec<Test>`. You can call it multiple times with the same map to accumulate tests across different paths.

---

## Working with results

`TestResult` exposes the following:

```rust
r.name()                 // &str — test name as written in the YAML key
r.passed()               // bool
r.get_failure_message()  // Option<&str> — first failure reason
r.file_path()            // Option<&PathBuf> — source file
r.duration_ms            // u64 — wall-clock time in milliseconds
r.assertions()           // Iterator<Item = &AssertionResult>
```

Each `AssertionResult` has:

```rust
assertion.name    // String — e.g. "status 200", "body.user.name"
assertion.passed  // bool
assertion.message // Option<String> — failure detail
```

Example — print all failed assertions:

```rust
for result in &results {
    if !result.passed() {
        println!("FAIL  {}", result.name());
        for a in result.assertions() {
            if !a.passed {
                println!("  ✗  {}", a.message.as_deref().unwrap_or(&a.name));
            }
        }
    }
}
```

---

## Console output

`run_path` and `run_from_yaml` produce no output. If you want the same formatted output as the CLI, call `print_test_results` after running:

```rust
use yapitest::{run_path, print_test_results};
use std::path::Path;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let t0 = Instant::now();
    let results = run_path(Path::new("./tests")).await?;
    print_test_results(&results, t0.elapsed().as_secs_f32(), 2);
    Ok(())
}
```

Verbosity levels match the CLI `-v` flag: `0` silent, `1` names only, `2` pass/fail summary (default), `3` full assertion detail.

---

## Exit codes

The library never calls `std::process::exit`. You control how failures are handled:

```rust
let any_failed = results.iter().any(|r| !r.passed());
std::process::exit(if any_failed { 1 } else { 0 });
```
