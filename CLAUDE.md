# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Yapitest

Yapitest is a YAML-based API testing framework. Instead of writing code-based test assertions, users define tests in YAML files. The project has three implementations: Python (primary/published), Rust (active development on `rust-new` branch), and Zig (early stage).

## Commands

### Python

```bash
# Install dependencies
pip install ruamel.yaml requests

# Run CLI
python -m yapitest.main <test-paths>

# Run tests
pytest                          # all tests
pytest testing/unit/test_foo.py # single file
```

The `pythonpath` is set to `./src` in `pyproject.toml`, so `pytest` resolves imports correctly from the project root.

### Rust (`rs/` directory)

```bash
cd rs
cargo build
cargo run -- <test-paths> [-g group] [-i include] [-x exclude] [-t threads]
cargo test
```

### Zig (`zig/` directory)

```bash
cd zig
zig build
zig build run
```

### Publishing (Python)

```bash
python -m flit publish
```

The GitHub Actions release workflow (`.github/workflows/release.yaml`) reads the version from `src/yapitest/__init__.py` and publishes to PyPI automatically on manual trigger.

## Architecture

### Python implementation (`src/yapitest/`)

**Entry point**: `main.py` — `YapProject` orchestrates discovery and execution. Writes `yapitest-results.json` on completion; exit code reflects pass/fail.

**Discovery** (`find/finder.py`): Recursively finds YAML files matching `test[-_].*\.ya?ml$` or `.*test(?:s)?\.ya?ml$` for tests, and `(yapitest-)?config\.ya?ml$` for configs.

**Config** (`test/config.py`): `ConfigData` supports hierarchical inheritance — configs in parent directories are chained automatically. Provides URLs, variables, step-sets, and environment variable interpolation.

**Test execution** (`test/test.py`, `test/step.py`):
- `Test`: a named test with optional setup/cleanup phases and an ordered list of `TestStep`s, optionally tagged with groups.
- `TestStep` / `StepSet`: an individual HTTP call, or a named reusable sequence of steps that can be referenced across tests.
- Variable substitution uses `$variable.path.syntax` — can reference config vars (`$vars.name`), prior step responses (`$step-id.response.field`), setup data (`$setup.token`), or config URLs (`$urls.default`).

**Assertions** (`test/assertions/`):
- `StatusCodeAssertion`: exact or wildcard status codes (`4xx`, `20x`)
- `BodyAssertion`: field-level assertions — exact value, type check (`+str`, `+int`, `+bool`, `+dict`, `+arr`, `+float`), or length (`len(field): >=10`)
- `FullAssertion`: Ensure that the BodyAssertion expected value has no missing fields

**Utilities** (`utils/`): `DeepDict` (`dict_wrapper.py`) enables nested dict access with dot-path `get()`.

### Rust implementation (`rs/src/`)

Mirrors the Python architecture:
- `main.rs`: CLI parsing (clap), test discovery (walkdir), config chaining, multi-threaded execution via `tokio` + `std::thread`
- `config.rs`: `ConfigData` with parent chaining via `Arc<RwLock<ConfigData>>`
- `test.rs`: `Test` + `TestResult`, assertion logic, result printing
- `test_step.rs`: `TestStep` with reqwest-based HTTP execution

The Rust binary adds a `-t <threads>` flag not present in Python — it splits tests into chunks across OS threads, each with its own `tokio` runtime.

## Test file format

Test YAML files (`test-*.yaml` or `*test.yaml`) define a list of named tests. Config YAML files (`yapitest-config.yaml` or `config.yaml`) in the same or parent directories apply to those tests. See `Tests.md` and `Configs.md` for full format documentation, and `testing/tests/` for working examples.
