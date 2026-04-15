# Yapitest

Yapitest is an API testing framework where tests are written entirely in YAML. Instead of writing assertion code, you describe HTTP requests and what you expect back — yapitest handles the rest.

---

## Installation

Install with [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html):

```bash
cargo install yapitest
```

---

## Quick start

Create a test file (name must start or end with `test`):

```yaml
# test-healthcheck.yaml

test-api-is-up:
  steps:
    - path: /api/healthz
      assert:
        status-code: 200
        body:
          healthy: true
```

Point yapitest at the file or its directory:

```bash
yapitest test-healthcheck.yaml
# or
yapitest path/to/tests/
```

---

## CLI reference

```
yapitest [paths...] [options]
```

| Argument | Description |
|----------|-------------|
| `paths` | One or more files or directories to search for tests. |
| `-g GROUP` | Only run tests that belong to this group. Repeatable. |
| `-i TEXT` | Only run tests whose name contains this substring. Repeatable. |
| `-x TEXT` | Skip tests whose name contains this substring. Repeatable. |
| `-t THREADS` | Number of parallel threads. Defaults to ~75% of available cores. Tests from the same file always run on the same thread. |
| `-v LEVEL` | Verbosity: `0` silent, `1` names only, `2` pass/fail (default), `3` full assertion detail. |
| `--output FILE` | Write a [CTRF](https://ctrf.io) JSON report to this path. |

### Examples

```bash
# Run all tests in a directory
yapitest tests/

# Run only tests in the "smoke" group
yapitest tests/ -g smoke

# Run only tests whose name contains "user"
yapitest tests/ -i user

# Run everything except tests tagged "slow"
yapitest tests/ -x slow

# Combine filters
yapitest tests/ -g smoke -i create

# Run across 4 threads
yapitest tests/ -t 4

# Full assertion output
yapitest tests/ -v 3

# Write a CTRF report
yapitest tests/ --output results.json
```

---

## File discovery

Yapitest recursively searches the given paths for:

- **Test files** — filenames starting or ending with `test` (case-insensitive), with a `.yaml` or `.yml` extension. Valid examples: `test-users.yaml`, `auth-tests.yaml`, `user_test.yml`.
- **Config files** — files named `yapitest-config.yaml`, `yapitest-config.yml`, `config.yaml`, or `config.yml`.

Config files are automatically applied to tests in the same directory and any subdirectory. See [Config Files](config.md) for details.

---

## Output

### Console

Yapitest prints a live result for each test as it runs:

```
yapitest v1.0.1
────────────────────────────────────────
Collecting tests...
Found 6 tests

  PASS  test-create-user
  PASS  test-get-profile
  FAIL  test-delete-post
  PASS  test-list-posts

────────────────────────────────────────
Results: 3 passed, 1 failed  (4 total, 0.42s)
```

Use `-v 3` to see individual assertion results:

```
  FAIL  test-delete-post
      ✓  status 403
      ✗  'message' — expected "Forbidden", got "Unauthorized"
```

### CTRF report

Use `--output <file>` to write a [CTRF](https://ctrf.io)-compatible JSON report:

```bash
yapitest tests/ --output results.json
```

```json
{
  "reportFormat": "CTRF",
  "specVersion": "1.0.0",
  "results": {
    "tool": { "name": "yapitest", "version": "1.0.1" },
    "summary": {
      "tests": 4, "passed": 3, "failed": 1,
      "skipped": 0, "pending": 0, "other": 0,
      "start": 1700000000000, "stop": 1700000001234
    },
    "tests": [
      { "name": "test-create-user", "status": "passed", "duration": 43 },
      { "name": "test-delete-post", "status": "failed", "duration": 18,
        "message": "'message' — expected \"Forbidden\", got \"Unauthorized\"" }
    ]
  }
}
```

### Exit codes

| Code | Meaning |
|------|---------|
| `0`  | All tests passed |
| `1`  | One or more tests failed |
