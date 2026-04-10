# Config Files

Config files centralise shared settings — base URLs, variables, and reusable step-sets — so tests stay concise and changes (like a staging URL) only need to be made in one place.

---

## File naming and discovery

A config file must be named one of:

- `yapitest-config.yaml`
- `yapitest-config.yml`
- `config.yaml`
- `config.yml`

Yapitest automatically discovers config files by walking up the directory tree from each test file. Every config found along the path is loaded and chained together. The config closest to the test file takes priority.

```
/project/
  yapitest-config.yaml          ← applied to all tests
  tests/
    yapitest-config.yaml        ← applied to tests in /tests/ only, inherits from parent
    auth/
      test-login.yaml           ← inherits from both configs above
    posts/
      yapitest-config.yaml      ← applied to tests in /tests/posts/ only
      test-posts.yaml
```

When a value (variable, URL, step-set) is looked up, yapitest searches from the test's own inline config outward through the config hierarchy. The first definition found wins.

---

## `vars`

Named variables accessible in tests as `$vars.<name>`.

### Literal value

```yaml
vars:
  sample-user: test-user-123
  base-path: /api/v2
```

### From an environment variable

```yaml
vars:
  api-key:
    env: API_KEY          # reads $API_KEY from the environment
```

If the environment variable is not set, an error is thrown.

### From an environment variable with a fallback

```yaml
vars:
  base-url:
    env: BASE_URL
    default: http://127.0.0.1:8181
```

If `$BASE_URL` is set, that value is used; otherwise `http://127.0.0.1:8181` is used.

### Using variables in tests

```yaml
- data:
    username: $vars.sample-user
    password: $vars.api-key
- path: /api/user/$vars.sample-user
```

---

## `urls`

Named base URLs. The special key `base` is the default URL used by all steps that do not specify a `url` field explicitly.

```yaml
urls:
  base: http://127.0.0.1:8181
  admin: http://admin.internal:9000
```

URLs can reference variables:

```yaml
vars:
  default-url:
    env: BASE_URL
    default: http://127.0.0.1:8181

urls:
  base: $vars.default-url
```

Use a named URL in a specific step with `url: $urls.<name>`:

```yaml
steps:
  - url: $urls.admin
    path: /api/admin/stats
    assert:
      status-code: 200
```

---

## `step-sets`

Reusable sequences of steps. Step-sets are the primary mechanism for sharing setup, teardown, and common action sequences across tests.

```yaml
step-sets:
  create-user:
    once: true
    steps:
      - id: create-user
        path: /api/user/create
        method: POST
        data:
          username: test-user
          password: secret123!
        assert:
          status-code: 200
          body:
            token: +str
    output:
      token:    $create-user.response.token
      username: $create-user.data.username
```

### `once`

When `once: true`, the step-set runs only once per yapitest session no matter how many tests reference it. The result is cached and reused by all subsequent callers.

This is most useful for expensive or destructive setup that should only happen once, like creating a shared test user.

```yaml
step-sets:
  seed-database:
    once: true
    steps:
      - path: /api/admin/seed
        method: POST
```

When `once` is omitted or `false`, the step-set runs fresh for every test that uses it.

### `steps`

The list of steps to execute, using the same format as [test steps](tests.md#steps). Step-sets may also reference other step-sets by name (nested composition):

```yaml
step-sets:
  create-user:
    steps:
      - id: user
        path: /api/user/create
        method: POST
        data:
          username: test-user
          password: secret!
    output:
      token: $user.response.token

  create-user-and-post:
    steps:
      - create-user             # inline reference to another step-set
      - id: post
        path: /api/post/create
        method: POST
        headers:
          API-Token: $create-user.token
        data:
          title: My Post
    output:
      token:   $create-user.token
      post_id: $post.response.post_id
```

### `output`

Maps string keys to values from the step-set's internal steps. These become the step-set's public interface — how calling tests access results.

```yaml
output:
  token:    $create-user.response.token    # from a step's response
  username: $create-user.data.username    # from a step's request body
  id:       $create-user.response.id
```

Output keys are referenced in tests as:

- `$setup.<key>` — when the step-set is used as `setup:`
- `$<step-set-name>.<key>` — when referenced inline in `steps:`

---

## Using step-sets in tests

### As `setup`

Runs before any test steps. The setup's outputs are accessible via `$setup.<key>`.

```yaml
test-get-profile:
  setup: create-user
  steps:
    - path: /api/user
      method: GET
      headers:
        API-Token: $setup.token
      assert:
        status-code: 200
        body:
          name: $setup.username
```

### As `cleanup` / `teardown`

Runs after test steps complete, even on failure. Use `cleanup` in the Python implementation and `teardown` in the Rust implementation.

```yaml
test-temporary-resource:
  setup:   create-resource     # Python
  cleanup: delete-resource     # Python
  # teardown: delete-resource  # Rust
  steps:
    - path: /api/resource/$setup.id
      assert:
        status-code: 200
```

### Inline in steps

A plain string in the `steps` list runs a step-set at that point. Outputs are then accessible via `$<step-set-name>.<key>`.

```yaml
test-post-flow:
  steps:
    - create-user                         # runs the step-set
    - path: /api/post/create
      method: POST
      headers:
        API-Token: $create-user.token     # access via step-set name
      data:
        title: Hello
      assert:
        status-code: 201
```

---

## Complete config example

```yaml
# yapitest-config.yaml

vars:
  sample-user: test-user-123
  base-url:
    env: BASE_URL
    default: http://127.0.0.1:8181

urls:
  base: $vars.base-url

step-sets:
  create-user:
    once: true
    steps:
      - id: create-user
        path: /api/user/create
        method: POST
        data:
          username: $vars.sample-user
          password: s3cr3t!
        assert:
          status-code: 200
          body:
            token: +str
    output:
      token:    $create-user.response.token
      username: $create-user.data.username

  delete-user:
    steps:
      - path: /api/user
        method: DELETE
        headers:
          API-Token: $setup.token
        assert:
          status-code: 200
```
