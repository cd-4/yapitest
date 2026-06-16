# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Yapitest

Yapitest is a YAML-based API testing framework. Instead of writing code-based test assertions, users define tests in YAML files describing HTTP requests and what to expect back.

This branch (`library`) contains the **Rust implementation** in `rs/`, which is the version published to crates.io. (Earlier Python and Zig implementations live on other branches and are not present here.)

## Commands

### Rust (`rs/` directory)

```bash
cd rs
cargo build
cargo run -- <test-paths> [-g group] [-i include] [-x exclude] [-k name] [-t threads] [-v level] [--output file.json]
cargo test                       # all unit tests
cargo test <substring>           # tests matching a name (one filter only)
```

The CLI flags: `-g` group filter, `-i` include-by-substring, `-x` exclude-by-substring, `-k` exact-name, `-t` thread count, `-v` verbosity (0–3), `--output` writes a CTRF JSON report. A run exits nonzero if any test fails.

### Integration tests (sample API)

`testing/tests/` holds runnable YAML tests; `testing/api/main.py` is a Flask app they run against.

```bash
# 1. start the sample API (Flask required: pip install flask)
cd testing/api && python main.py            # serves on http://127.0.0.1:8181
# 2. in another shell, point the built binary at the tests
./rs/target/debug/yapitest testing/tests/test_dir
```

By convention, tests whose name ends in `-fail` are *expected* to fail — they exercise the framework's failure detection, so a full run reports them as failures intentionally.

### Docs site

User documentation is an MkDocs (Material) site under `docs/`.

```bash
mkdocs build            # build to ./site (gitignored); validates internal links
mkdocs serve            # live preview
```

### Publishing

Publishing is the Rust crate via `.github/workflows/publish-crate.yml`, which runs `cargo publish` from `rs/` on manual dispatch (so it uses the version in `rs/Cargo.toml`). `docs.yml` deploys the MkDocs site.

## Architecture (`rs/src/`)

- **`main.rs`** — CLI parsing (clap), test discovery (walkdir), config chaining, multi-threaded execution via `tokio` + `std::thread` (the `-t` flag splits tests into chunks across OS threads, each with its own `tokio` runtime). Prints per-test results, writes a CTRF JSON report only when `--output` is given, and exits nonzero if any test fails.
- **`config.rs`** — `ConfigData` with hierarchical parent chaining via `Arc<RwLock<ConfigData>>`; provides `vars`, `urls`, and `step-sets`. `TestStepGroup` runs a step-set (with `once` caching) and resolves its `output` block; `run_with_args` injects an `$args.*` scope for parameterized invocations.
- **`test.rs`** — `Test` + `TestResult`. `setup`/`teardown` deserialize as `StepSetInvocation` (a bare name, or `{name, args}`); assertion logic and result printing live here.
- **`test_step.rs`** — `TestStep` with reqwest-based HTTP execution, the interpolation engine, variable resolution, and the assertion comparison functions. This is the largest module.

### Interpolation engine (`test_step.rs`)

A single scanner (`scan_template`) backs two helpers reused everywhere a string can appear (headers, `query`, `data`, paths, assertion-expected values, step-set `output`):

- `interpolate_string` → always returns a `String` (headers, paths, query).
- `resolve_value` → preserves the JSON type when the template is exactly one reference, otherwise stringifies (data values, output).

Reference forms: bare `$path` and delimited `${path}`, usable inline; `$$` is a literal `$`. `get_variable` resolves config `vars`/`urls`, prior-step `response`/`data`, `setup`/inline-step-set outputs, and `$args`. `get_field` navigates dot-paths through objects **and** arrays (numeric segments index, e.g. `posts.0.id`).

### Assertions (`test_step.rs`)

- **status-code** (`check_status_code`): exact, wildcard (`4xx`, `20x`), or a list (`[200, 201]`, may mix wildcards).
- **body** (`compare_*`): exact value; type checks (`+str`, `+int`, `+float`, `+bool`, `+arr`, `+dict`, `+null`); presence markers (`+exists`, `+absent`); numeric value comparisons (`">=1"`, same operators as `len`); regex (`re/<pattern>`); length (`len(field): ">=10"`); array membership (`{+exists: {<partial object>}}`); and variable references.
- **headers**: same vocabulary applied to response headers (case-insensitive names; repeated headers exposed as arrays).
- **full**: every response body field must be asserted (no extras).
- **duration**: request completed within a time limit.

Request steps also support `wait-before` / `wait-after` / `retry` for polling.

## Test file format

Test YAML files (`test-*.yaml` or `*test.yaml`) define a map of named tests. Config YAML files (`yapitest-config.yaml` or `config.yaml`) in the same or parent directories apply to those tests.

Documentation lives in two parallel sets that must be kept in sync: root `Tests.md` / `Configs.md`, and the published MkDocs site `docs/tests.md`, `docs/config.md`, `docs/variables.md`, `docs/index.md`. Working examples are in `testing/tests/`. Design specs and implementation plans are under `docs/superpowers/`.
