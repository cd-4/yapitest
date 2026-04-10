# Config Files

Config files store shared variables, base URLs, and reusable step-sets that can be referenced across many tests. They reduce duplication and make it easy to change settings (like a base URL) in one place.

## File discovery

Config files are named `yapitest-config.yaml` or `config.yaml`. Yapitest automatically loads configs from the test file's directory and all parent directories, chaining them together:

```
yapitest-config.yaml        ← applied to all tests
  test_dir/
    config.yaml             ← applied to tests in test_dir/
    test_something.yaml
  another_test_dir/
    config.yaml             ← applied to tests in another_test_dir/
    another-test.yaml
```

When a config value is needed, yapitest searches from the test's own inline config outward through the directory hierarchy. The closest definition wins.

Configs can also be defined inline inside a test using the `config:` key — see [Tests](tests.md).

---

## `vars`

Variables can be referenced anywhere in tests as `$vars.<name>`.

```yaml
# yapitest-config.yaml

vars:
  # Literal value
  sample-user: test-user-123

  # From environment variable, with a fallback default
  base-url:
    env: BASE_URL
    default: http://127.0.0.1:8181

  # From environment variable only — error if unset and no default
  api-secret:
    env: API_SECRET
```

Use them in tests:

```yaml
- path: /api/user/$vars.sample-user
- data:
    password: $vars.api-secret
```

---

## `urls`

Named base URLs for your API requests. The special key `base` is used as the default URL for all steps that don't specify a `url` explicitly.

```yaml
urls:
  base: $vars.base-url
  admin: http://admin.example.com
```

Reference a specific URL in a step with `url: $urls.admin`.

---

## `step-sets`

Reusable sequences of steps that can be referenced in tests. This is the primary mechanism for sharing setup/teardown logic.

```yaml
step-sets:
  create-user:
    once: false     # if true, only runs once per yapitest run regardless of how many tests use it
    steps:
      - id: create-user
        path: /api/user/create
        method: POST
        data:
          username: test-user
          password: test-password
        assert:
          status-code: 200
          body:
            token: +str
    output:
      token: $create-user.response.token
      username: $create-user.data.username
```

### `once`

When `once: true`, the step-set runs only once per yapitest session, no matter how many tests reference it. The result is cached and reused. This is useful for expensive setup that is safe to share across tests.

### `output`

Maps output keys to values from the step-set's internal steps. These outputs are accessible in tests that use the step-set via `$setup.<key>` (when used as `setup:`) or `$<step-set-name>.<key>` (when inlined).

### Using step-sets in tests

**As `setup` / `teardown`:**

```yaml
test-example:
  setup: create-user
  teardown: delete-user
  steps:
    - path: /api/user
      headers:
        API-Token: $setup.token
      assert:
        status-code: 200
        body:
          name: $setup.username
```

**Inline in `steps`:**

```yaml
test-example:
  steps:
    - create-user          # step-set name as a plain string
    - path: /api/user
      headers:
        API-Token: $create-user.token
      assert:
        status-code: 200
```
