# Config Files

Config files centralise shared settings — base URLs, variables, and reusable step-sets — so tests stay concise and a single change (like a staging URL) propagates everywhere.

---

## File naming and discovery

A config file must be named one of:

- `yapitest-config.yaml`
- `yapitest-config.yml`
- `config.yaml`
- `config.yml`

Yapitest automatically discovers config files by walking up the directory tree from each test file. Every config found along the path is loaded and chained. The config closest to the test file takes priority.

```
/project/
  yapitest-config.yaml           ← applied to all tests
  tests/
    yapitest-config.yaml         ← applied to tests in /tests/, inherits from parent
    auth/
      test-login.yaml            ← inherits from both configs above
    posts/
      yapitest-config.yaml       ← applied to tests in /tests/posts/ only
      test-posts.yaml
```

When a value is looked up, yapitest searches from the test's own inline config outward through the hierarchy. The first definition found wins.

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
    env: API_KEY          # reads $API_KEY at runtime; errors if not set
```

### From an environment variable with a fallback

```yaml
vars:
  base-url:
    env: BASE_URL
    default: http://127.0.0.1:8181
```

If `$BASE_URL` is set it is used; otherwise `http://127.0.0.1:8181` is used.

### Generated from a regex pattern

Prefix a value with `re/` to generate a random string matching that pattern each time the variable is resolved. A new value is generated fresh for each test step that uses it.

```yaml
vars:
  username: "re/user-[a-z]{6}"       # e.g. "user-kqmvtj"
  order-ref: "re/ORD-[0-9]{8}"       # e.g. "ORD-48291057"
```

The same `re/` prefix works directly in step `data` fields as well:

```yaml
- path: /api/user/create
  method: POST
  data:
    username: "re/[a-z]{8}"
    ref: "re/REF-[0-9]{6}"
```

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
          username: $vars.sample-user
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

When `once: true`, the step-set runs only once per yapitest session regardless of how many tests reference it. The result is cached and reused by all subsequent callers.

Ideal for expensive or stateful setup that should only happen once per run, such as creating a shared test user or seeding a database.

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

The list of steps to execute, in the same format as [test steps](tests.md#steps). Step-sets can reference other step-sets by name for nested composition:

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
  token:    $create-user.response.token
  username: $create-user.data.username
  id:       $create-user.response.id
```

Output values use the full [interpolation syntax](variables.md#syntax), so they
can compose literal text with references and keep the resolved type for
whole-value references:

```yaml
output:
  auth-header: "Bearer $create-user.response.token"   # inline → a string
  id:          $create-user.response.id               # whole-value → stays an integer
```

Output keys are referenced as:

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

#### Parameterized step-sets (`args`)

Instead of a bare name, `setup` (and `teardown`) may be a map with `name` and `args`. Each arg is resolved in the calling test's scope and exposed inside the step-set as `$args.<key>`. This lets one step-set serve many cases — instead of writing near-identical `login-alice`, `login-bob`, `login-carol` step-sets:

```yaml
# the shared, parameterized step-set:
step-sets:
  login:
    once: false        # parameterized invocations always run fresh (see below)
    steps:
      - id: do-login
        path: /api/user/login
        method: POST
        data:
          username: $args.username
          password: $args.password
    output:
      token: $do-login.response.token
```

```yaml
# a test that supplies args:
test-login-as-alice:
  setup:
    name: login
    args:
      username: alice@example.com
      password: $vars.alice-password    # args resolve in the caller's scope
  steps:
    - path: /api/user/whoami
      headers:
        Authorization: "Bearer ${setup.token}"
      assert:
        body:
          name: alice@example.com
```

A parameterized invocation always runs fresh, ignoring `once: true`, because different args are expected to produce different results.

### As `teardown`

Runs after test steps complete, even if the test failed. If `setup` fails, teardown is not run.

```yaml
test-temporary-resource:
  setup:    create-resource
  teardown: delete-resource
  steps:
    - path: /api/resource/$setup.id
      assert:
        status-code: 200
```

### Inline in steps

A plain string in the `steps` list runs a step-set at that point. Outputs are accessible via `$<step-set-name>.<key>`.

```yaml
test-post-flow:
  steps:
    - create-user
    - path: /api/post/create
      method: POST
      headers:
        API-Token: $create-user.token
      data:
        title: Hello
      assert:
        status-code: 201
```

---

## Complete example

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
