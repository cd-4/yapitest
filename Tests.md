# Tests

A test file is any YAML file whose name starts or ends with `test` (e.g. `test-users.yaml`, `auth-tests.yaml`). Each top-level key that starts or ends with `test` (case-insensitive) is treated as a named test. All other keys are ignored.

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
  teardown:  step-set-name      # optional
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
yapitest tests/ -g regression   # runs test-checkout (and any other test tagged "regression")
```

A test is included if it belongs to *any* of the specified groups.

### `config`

An inline config block scoped to the test. Follows the same structure as a [config file](./Configs.md). Inline config takes priority over any external config files.

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

Names a [step-set](./Configs.md#step-sets) to run before the test's steps. If setup fails, the test fails immediately and no steps run.

The setup result is accessible in steps via `$setup.<output-key>`.

```yaml
test-create-post:
  setup: create-user
  steps:
    - path: /api/post/create
      headers:
        API-Token: $setup.token    # "token" is an output of the create-user step-set
```

### `teardown`

Names a step-set to run after the test's steps complete — even if the test failed. If setup fails, teardown is **not** run.

```yaml
test-create-and-delete:
  setup:    create-user
  teardown: delete-user
  steps:
    - path: /api/user
      assert:
        status-code: 200
```

### `steps`

An ordered list of HTTP steps (or inline step-set references) that make up the test. Steps run in order. If a step fails, the test stops immediately.

---

## Steps

Each entry in `steps` is either a **step object** or a **step-set reference**.

### Step-set references

A plain string in the steps list runs a named step-set inline, as if its steps were inserted at that point. The step-set's outputs are then accessible via `$<step-set-name>.<key>`.

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

### Step fields

#### `id` *(optional)*

A name for the step. Required if you want to reference this step's data or response in later steps.

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

The URL path for the request, appended to the `urls.base` value from config. Path segments may contain variable references.

```yaml
- path: /api/user/$setup.username
- path: /api/post/$create-post.response.post_id
```

A leading `/` is added automatically if missing.

#### `url` *(optional)*

Override the base URL for this step. Can be a literal URL or a `$urls.<name>` reference. If omitted, `urls.base` from the nearest config is used.

```yaml
- url: http://admin.internal
  path: /api/admin/users

- url: $urls.secondary
  path: /api/status
```

#### `method` *(optional)*

HTTP method. Defaults to `GET` if omitted. Case-insensitive.

```yaml
method: POST    # GET, PUT, PATCH, DELETE, etc.
```

#### `headers` *(optional)*

A map of header names to values. Values may be variable references.

```yaml
headers:
  API-Token: $setup.token
  Accept: application/json
  X-Request-ID: $vars.request-id
```

#### `data` *(optional)*

The JSON body to send with the request. Nested maps and arrays are supported. String values may be variable references.

```yaml
data:
  username: alice
  role: admin
  settings:
    theme: dark
    notifications: true
```

You can also use a `re/<pattern>` string to generate a random value matching a regex pattern:

```yaml
data:
  username: "re/[a-z]{8}"          # generates e.g. "kqmvtjzr"
  reference-id: "re/REF-[0-9]{6}"  # generates e.g. "REF-482910"
```

Or pass an entire step response or variable as the body:

```yaml
data: $create-user.response        # entire response object
data: $vars.default-payload        # a variable that holds an object
```

#### `assert` *(optional)*

Assertions to check against the HTTP response. See [Assertions](#assertions) below.

---

## Variables

Any string value starting with `$` is treated as a variable reference and resolved before the request is sent. References work in `path` segments, `url`, `headers` values, `data` values at any depth, and assertion `body` expected values.

### Syntax

```
$<namespace>.<key>[.<nested-key>...]
```

### `$vars.<name>`

A variable defined in a config file or inline test config.

```yaml
data:
  username: $vars.sample-user
  password: $vars.api-key
```

### `$urls.<name>`

A URL defined in a config file.

```yaml
- url: $urls.admin
  path: /api/admin/report
```

### `$<step-id>.response.<field>`

The JSON response body of a named step. Supports arbitrary nesting.

```yaml
- id: create-post
  path: /api/post/create
  method: POST
  ...

- path: /api/post/$create-post.response.post_id
```

```yaml
- id: login
  path: /api/login
  ...

- path: /api/dashboard
  headers:
    Authorization: $login.response.auth.token    # nested field
```

### `$<step-id>.data.<field>`

The JSON **request body** of a named step.

```yaml
- id: create-user
  path: /api/user/create
  method: POST
  data:
    username: alice

- path: /api/user/$create-user.data.username    # "alice"
```

### `$setup.<key>`

An output key from the step-set used as `setup:`. Output keys are defined in the step-set's [`output`](./Configs.md#output) block.

```yaml
test-example:
  setup: create-user
  steps:
    - path: /api/post/create
      headers:
        API-Token: $setup.token
      data:
        author: $setup.username
```

### `$<step-set-name>.<key>`

When a step-set is referenced inline in `steps`, its outputs are accessible by the step-set's name.

```yaml
steps:
  - create-user
  - path: /api/post/create
    headers:
      API-Token: $create-user.token
```

### Resolution order

When a reference like `$foo.bar` is encountered, yapitest resolves it in this order:

1. **Config values** — checks if `foo` is `vars` or `urls`
2. **Prior steps** — checks if `foo` matches the `id` of a step that has already run
3. **Setup/step-set outputs** — checks if `foo` matches `setup` or an inline step-set name

If none match, an error is thrown and the test fails immediately.

---

## Assertions

The `assert` block can contain any combination of `status-code`, `body`, `full`, and `duration`.

```yaml
assert:
  status-code: 201
  full: true
  duration: 500ms
  body:
    id: +int
    title: +str
    published: false
```

### `status-code`

**Exact match:**

```yaml
assert:
  status-code: 200
```

**Wildcard match** — use `x` as a digit placeholder:

```yaml
assert:
  status-code: 4xx    # matches 400–499
  status-code: 20x    # matches 200–209
  status-code: 2xx    # matches 200–299
```

---

### `body`

Asserts fields within the JSON response body. By default, fields not listed are ignored — see `full` to change this.

#### Exact value

```yaml
assert:
  body:
    status: "active"
    verified: true
    count: 0
```

#### Nested fields

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

Use `+type` to assert that a field exists and has the correct type, without checking its value.

| Assertion | Matches |
|-----------|---------|
| `+str` or `+string` | String |
| `+int` or `+integer` | Integer |
| `+float` or `+flt` | Float |
| `+bool` or `+boolean` | Boolean |
| `+arr`, `+array`, or `+list` | Array |
| `+dict`, `+dic`, `+dictionary`, or `+map` | Object |

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

#### Variable references

An expected value can be a variable reference. The response field must equal the resolved value.

```yaml
assert:
  body:
    username: $setup.username
    role: $vars.expected-role
    post_id: $create-post.response.id
```

#### Regex assertions

Use `re/<pattern>` to assert that a string field matches a regular expression.

```yaml
assert:
  body:
    token: "re/[A-Za-z0-9]{32}"      # exactly 32 alphanumeric chars
    slug: "re/[a-z0-9-]+"            # lowercase slug
    created_at: "re/\\d{4}-\\d{2}-\\d{2}"   # ISO date format
```

#### Size assertions (`len`)

Use `len(field)` as the assertion key to check the length of a string, array, or object.

```yaml
assert:
  body:
    token: +str
    len(token): 32          # exactly 32 characters
    len(token): '>=8'       # at least 8 characters
    len(token): '<=64'      # at most 64 characters
    items: +arr
    len(items): '>=1'       # at least one item
    len(items): '>0'        # more than zero items
```

Supported operators: `=`, `>=`, `<=`, `>`, `<`.

---

### `full`

When `full: true`, every field in the response body must be explicitly listed in `body`. Any field present in the response but absent from the assertion fails the test.

```yaml
assert:
  full: true
  body:
    id: +int
    name: +str
    email: +str
    created_at: +str
```

Useful for detecting when an API starts returning unexpected fields (leaked internal IDs, passwords, etc.) or when a response schema changes unexpectedly.

---

### `duration`

Asserts that the HTTP request completed within a time limit. Accepts milliseconds as an integer, or a string with a `ms` or `s` suffix.

```yaml
assert:
  duration: 500       # must complete in under 500ms
  duration: 500ms     # same
  duration: 2s        # must complete in under 2 seconds
```

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
        ref: "re/REF-[0-9]{6}"   # randomly-generated reference ID
      assert:
        status-code: 201
        duration: 1s
        body:
          post_id: +int

    - path: /api/post/$new-post.response.post_id
      method: GET
      assert:
        status-code: 200
        duration: 500ms
        full: true
        body:
          id: +int
          title: "Hello World"
          body: "My first post"
          user_id: +int

test-delete-requires-auth:
  steps:
    - path: /api/post/1
      method: DELETE
      assert:
        status-code: 4xx

test-pagination:
  steps:
    - path: /api/post/list
      assert:
        status-code: 200
        body:
          posts: +arr
          len(posts): '<=20'
```
