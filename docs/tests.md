# Tests

A test file is any YAML file whose name starts or ends with `test` (e.g. `test-users.yaml`, `auth-tests.yaml`). Each key in the file whose name starts or ends with `test` (case-insensitive) is treated as a named test. All other keys are ignored.

```yaml
# test-users.yaml

config:               # optional file-level config (see Config Files)
  vars:
    base-password: secret123

test-create-user:     # ← a test (name starts with "test")
  steps:
    - path: /api/user/create
      method: POST
      data:
        username: alice
        password: $vars.base-password
      assert:
        status-code: 200

user-test:            # ← also a valid test name (ends with "test")
  steps: ...
```

---

## Test structure

```yaml
test-name:
  groups:    [tag1, tag2]       # optional
  config:    { ... }            # optional inline config
  setup:     step-set-name      # optional
  steps:                        # required
    - ...
  cleanup:   step-set-name      # optional (Python); use `teardown` in Rust
```

### `groups`

An optional list of string tags. Used with the `-g` CLI flag to run only matching tests.

```yaml
test-checkout:
  groups:
    - staging
    - regression
  steps: ...
```

```bash
yapitest tests/ -g regression   # runs test-checkout (and any other test with "regression")
```

A test is included if it belongs to *any* of the specified groups.

### `config`

An inline config block scoped to the test. Follows the same structure as a [config file](config.md). Inline config takes priority over any external config files.

```yaml
test-something:
  config:
    vars:
      my-password: s3cr3t
    urls:
      base: http://staging.example.com
  steps:
    - path: /api/login
      data:
        password: $vars.my-password
```

### `setup`

Names a [step-set](config.md#step-sets) to run before the test's steps. If setup fails, the test fails immediately and no steps run.

The setup result is accessible in steps via `$setup.<output-key>`.

```yaml
test-create-post:
  setup: create-user
  steps:
    - path: /api/post/create
      headers:
        API-Token: $setup.token    # "token" is an output of the create-user step-set
```

### `cleanup` / `teardown`

Names a step-set to run after the test's steps complete — even if the test failed. Use `cleanup` in the Python implementation and `teardown` in the Rust implementation.

If setup fails, cleanup is **not** run. If any step fails, cleanup still runs.

```yaml
test-create-and-delete:
  setup:   create-user
  cleanup: delete-user     # Python
  # teardown: delete-user  # Rust
  steps:
    - path: /api/user
      assert:
        status-code: 200
```

### `steps`

An ordered list of HTTP steps (or inline step-set references) that make up the test body. Steps run in order. If a step fails, the test stops and subsequent steps are marked as skipped.

---

## Steps

Each entry in `steps` is either a **step object** or a **step-set reference**.

### Step-set references

A plain string in the steps list runs a named step-set inline, as if its steps were inserted at that point. The step-set's outputs become accessible via `$<step-set-name>.<key>`.

```yaml
test-example:
  steps:
    - create-user               # runs the "create-user" step-set
    - path: /api/user
      headers:
        API-Token: $create-user.token
      assert:
        status-code: 200
```

### Step object fields

#### `id` *(optional)*

A name for the step. Required if you want to reference this step's request data or response in later steps via `$<id>.response.<field>` or `$<id>.data.<field>`.

```yaml
- id: login
  path: /api/user/login
  method: POST
  data:
    username: alice
    password: secret

- path: /api/user/profile
  headers:
    Authorization: $login.response.token
```

#### `path` *(required)*

The URL path to request. Appended to the `urls.base` value from config. Path segments may contain variable references.

```yaml
- path: /api/user/$setup.username      # variable in path segment
- path: /api/post/$create-post.response.post_id
```

A leading `/` is added automatically if missing.

#### `url` *(optional)*

Override the base URL for this step. Can be a literal URL string or a `$urls.<name>` reference. If omitted, `urls.base` from the nearest config is used.

```yaml
- url: http://admin.internal
  path: /api/admin/users

- url: $urls.secondary
  path: /api/status
```

#### `method` *(optional)*

HTTP method. Defaults to `GET` if omitted. Case-insensitive.

```yaml
method: POST    # or GET, PUT, PATCH, DELETE, etc.
```

#### `headers` *(optional)*

A mapping of header names to values. Values may contain variable references.

```yaml
headers:
  API-Token: $setup.token
  Accept: application/json
  X-Request-ID: $vars.request-id
```

#### `data` *(optional)*

The JSON body sent with the request. Nested maps and lists are supported. String values may contain variable references.

```yaml
data:
  username: alice
  role: admin
  settings:
    theme: dark
    notifications: true
```

You can also pass an entire step response or variable as the body:

```yaml
data: $create-user.response       # entire response object
data: $vars.default-payload       # a variable that holds an object
```

#### `assert` *(optional)*

Assertions to check against the HTTP response. See [Assertions](#assertions) below.

---

## Assertions

The `assert` block can contain any combination of `status-code`, `body`, and `full`.

```yaml
assert:
  status-code: 201
  full: true
  body:
    id: +int
    title: +str
    published: false
```

### `status-code`

Asserts the HTTP response status code.

**Exact match** — provide an integer:

```yaml
assert:
  status-code: 200
```

**Wildcard match** — use `x` as a digit wildcard:

```yaml
assert:
  status-code: 4xx    # matches 400–499
  status-code: 20x    # matches 200–209
  status-code: 2xx    # matches 200–299
```

---

### `body`

Asserts fields within the JSON response body. By default, unspecified fields in the response are ignored (see `full` to change this).

The value for each key may be:

- **An exact value** — the field must equal this exactly
- **A type assertion** (`+type`) — the field must exist and be of the given type
- **A variable reference** (`$...`) — the field must equal the resolved variable value
- **A size assertion key** (`len(field)`) — assert the length of the named field

#### Exact value match

```yaml
assert:
  body:
    status: "active"
    verified: true
    count: 0
```

#### Nested fields

Body assertions can traverse nested objects using YAML nesting:

```yaml
assert:
  body:
    user:
      name: "Alice"
      role: "admin"
    meta:
      page: 1
```

#### Type assertions

Use `+type` to assert a field exists and is the right type, without checking its value.

| Assertion | Accepted types |
|-----------|---------------|
| `+str` or `+string` | String |
| `+int` or `+integer` | Integer |
| `+float` or `+flt` | Float / decimal |
| `+bool` or `+boolean` | Boolean |
| `+arr`, `+array`, or `+list` | Array |
| `+dict`, `+dic`, `+dictionary`, or `+map` | Object / dictionary |

```yaml
assert:
  body:
    id: +int
    token: +str
    tags: +arr
    metadata: +dict
    score: +float
    active: +bool
```

#### Variable references in assertions

An expected value can be a variable reference. The field must equal the resolved value.

```yaml
assert:
  body:
    username: $setup.username
    role: $vars.expected-role
    post_id: $create-post.response.id
```

#### Size assertions (`len`)

Use `len(field)` as the key to assert the length of a string, array, or object field. The value is either an exact integer or a comparison string.

```yaml
assert:
  body:
    token: +str
    len(token): 20          # exactly 20 characters
    len(token): '<=20'      # at most 20 characters
    len(token): '>=8'       # at least 8 characters
    len(token): '>0'        # more than 0 characters
    items: +arr
    len(items): '>=1'       # at least one item
```

Supported operators: `=`, `>=`, `<=`, `>`, `<`.

---

### `full`

When `full: true`, the assertion checks that the response body contains **exactly** the fields listed in `body` — no more, no less. Any field present in the response but absent from `body` will fail the test.

```yaml
assert:
  full: true
  body:
    id: +int
    name: +str
    email: +str
    created_at: +str
```

This is useful for detecting when an API starts leaking fields it shouldn't (passwords, internal IDs, etc.) or when a response schema changes unexpectedly.

---

## Complete example

```yaml
# test-posts.yaml

test-create-and-get-post:
  setup: create-user          # defined in yapitest-config.yaml
  steps:
    - id: new-post
      path: /api/post/create
      method: POST
      headers:
        API-Token: $setup.token
      data:
        title: "Hello World"
        body: "My first post"
      assert:
        status-code: 201
        body:
          post_id: +int

    - path: /api/post/$new-post.response.post_id
      method: GET
      assert:
        status-code: 200
        full: true
        body:
          id: +int
          title: "Hello World"
          body: "My first post"
          user_id: +int

test-delete-needs-auth:
  steps:
    - path: /api/post/1
      method: DELETE
      assert:
        status-code: 403

test-pagination:
  steps:
    - path: /api/post/list
      assert:
        status-code: 200
        body:
          posts: +arr
          len(posts): '<=20'
```
