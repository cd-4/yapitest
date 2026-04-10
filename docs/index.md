# Yapitest

Yapitest (Yaml API Testing) is an API testing framework composed entirely of YAML files. Instead of writing assertion code, you describe HTTP requests and what you expect back — yapitest does the rest.

!!! note "Alpha"
    Yapitest is still in alpha and there may be some bugs. Feel free to open a [Pull Request](https://github.com/cd-4/yapitest/pulls) or submit an [issue](https://github.com/cd-4/yapitest/issues).

---

## Installation

```bash
cargo install --git https://github.com/cd-4/yapitest rs
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

Run it, pointing yapitest at the file or the directory containing it:

```bash
yapitest test-healthcheck.yaml
# or
yapitest path/to/tests/
```

---

## CLI reference

```
yapitest [paths...] [-g GROUP] [-i INCLUDE] [-x EXCLUDE]
```

| Argument | Description |
|----------|-------------|
| `paths` | One or more files or directories to search for tests. If omitted, the current directory is used. |
| `-g GROUP` | Only run tests that belong to this group. Can be repeated to match any of multiple groups. |
| `-i INCLUDE` | Only run tests whose name contains this substring. Can be repeated; a test is included if its name matches *any* of the strings. |
| `-x EXCLUDE` | Skip tests whose name contains this substring. Can be repeated; a test is excluded if its name matches *any* of the strings. |

**Rust binary only** (`rs/`):

| Argument | Description |
|----------|-------------|
| `-t THREADS` | Number of parallel threads to use. Defaults to 1 (sequential). Tests from the same file always run sequentially on the same thread. |

### Examples

```bash
# Run all tests in a directory
yapitest tests/

# Run only tests in the "smoke" group
yapitest tests/ -g smoke

# Run only tests whose name contains "user"
yapitest tests/ -i user

# Run everything except tests containing "slow"
yapitest tests/ -x slow

# Combine filters
yapitest tests/ -g smoke -x teardown

# Rust: run in parallel across 4 threads
./rs/target/release/rs tests/ -t 4
```

---

## File discovery

Yapitest recursively searches the given paths for:

- **Test files** — filenames matching `test[-_]*.yaml`, `test[-_]*.yml`, `*test.yaml`, `*tests.yaml` (case-insensitive). Example valid names: `test-users.yaml`, `user_test.yaml`, `auth-tests.yaml`.
- **Config files** — files named `yapitest-config.yaml`, `yapitest-config.yml`, `config.yaml`, or `config.yml`.

Config files are automatically applied to tests in the same directory or any subdirectory. See [Config Files](config.md) for details.

---

## Output

### Console

Each test prints a live status line while running, then settles to `PASS` or `FAIL`.

### `yapitest-results.json`

After every run, yapitest writes `yapitest-results.json` in the current working directory. The structure is:

```json
{
  "tool": "yapitest",
  "summary": {
    "start": 1700000000000,
    "stop":  1700000001234,
    "tests": 12,
    "passed": 11,
    "failed": 1,
    "pending": 0,
    "skipped": 0,
    "other": 0
  },
  "tests": [
    {
      "name": "test-create-user",
      "status": "passed",
      "duration": 43,
      "type": "API",
      "extra": [
        {
          "step": "post /api/user/create",
          "status": "passed",
          "assertions": [
            { "passed": true, "message": "Status code 200 == 200" },
            { "passed": true, "message": "token is +str" }
          ]
        }
      ]
    }
  ]
}
```

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | All tests passed |
| `1` | One or more tests failed |
