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

A plain string in the steps list runs a named step-set inline. The step-set's outputs are accessible via `$<step-set-name>.<key>`.

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

The URL path for the request, appended to the `urls.base` value from config. Path segments may contain variable references. A leading `/` is added automatically if missing.

```yaml
- path: /api/user/$setup.username
- path: /api/post/$create-post.response.post_id
```

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

A map of header names to values. Values may contain [variable references](variables.md), including inline (`Bearer ${setup.token}`).

```yaml
headers:
  API-Token: $setup.token
  Authorization: "Bearer ${setup.token}"
  Accept: application/json
  X-Request-ID: $vars.request-id
```

#### `query` *(optional)*

A map of query-string parameters. Values may contain variable references and are percent-encoded automatically, so this is the recommended way to pass query parameters — rather than writing them into `path`, where encoding is undefined. If `path` already contains a `?…` query string, these entries are appended to it.

```yaml
- path: /api/user/search
  query:
    q: $setup.username
    page: "2"
    sort: created_at
```

#### `data` *(optional)*

The JSON body to send with the request. Nested maps and arrays are supported. String values may be variable references, and `re/<pattern>` strings generate random matching values.

```yaml
data:
  username: alice
  role: admin
  ref: "re/REF-[0-9]{6}"     # generates e.g. "REF-482910"
  settings:
    theme: dark
    notifications: true
```

You can also pass an entire step response or variable as the body:

```yaml
data: $create-user.response       # entire response object
data: $vars.default-payload       # a variable holding an object
```

#### `assert` *(optional)*

Assertions to check against the HTTP response. See [Assertions](#assertions) below.

#### `wait-before` *(optional)*

Duration to sleep before the step runs. Accepts a bare integer (milliseconds), or a string with a `ms` or `s` suffix. Applies once before the first attempt — does **not** repeat between retries.

```yaml
wait-before: 500       # 500 milliseconds
wait-before: "500ms"   # same
wait-before: "2s"      # 2 seconds
```

#### `wait-after` *(optional)*

Duration to sleep after the step completes — whether it passed or failed (after all retries). Same format as `wait-before`. Useful for rate-limiting or letting downstream state propagate before the next step runs.

```yaml
wait-after: 1s
```

#### `retry` *(optional)*

Number of additional attempts to make if the step's assertions fail. Default: `0` (no retries). On each attempt the full HTTP request and all assertions are re-run. `wait-before` and `wait-after` do not repeat between attempts. If all attempts fail, the test stops immediately with the last failure.

```yaml
retry: 3    # up to 4 total attempts (1 initial + 3 retries)
```

```yaml
# Combining wait and retry: poll a slow endpoint
- path: /api/job/$create-job.response.id/status
  wait-before: 2s     # give the job time to start
  retry: 5            # retry up to 5 times if the status assertion fails
  wait-after: 500ms   # brief pause before the next step
  assert:
    status-code: 200
    body:
      status: "complete"
```

---

## Assertions

The `assert` block can contain any combination of `status-code`, `headers`, `body`, `full`, and `duration`.

```yaml
assert:
  status-code: 201
  duration: 500ms
  full: true
  headers:
    content-type: "re/application/json.*"
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

**List match** — pass if the actual code equals any element. Exact codes and wildcards can be mixed, which is handy for endpoints with more than one acceptable outcome:

```yaml
assert:
  status-code: [200, 201]
  status-code: [200, "4xx"]
```

---

### `body`

Asserts fields within the JSON response body. By default, extra fields in the response are ignored — see [`full`](#full) to change this.

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

Use `+type` to assert a field exists and has the correct type, without checking its value.

| Assertion | Matches |
|-----------|---------|
| `+str` or `+string` | String |
| `+int` or `+integer` | Integer |
| `+float` or `+flt` | Float |
| `+bool` or `+boolean` | Boolean |
| `+arr`, `+array`, or `+list` | Array |
| `+dict`, `+dic`, `+dictionary`, or `+map` | Object |
| `+null` or `+nil` | JSON `null` (field present **and** null) |

```yaml
assert:
  body:
    id: +int
    token: +str
    tags: +arr
    metadata: +dict
    score: +float
    active: +bool
    ends_at: +null
```

#### Presence assertions (`+exists` / `+absent`)

Assert whether a field is present, independent of its value. Useful for security-contract tests ("the public profile must not leak `password_hash`") without having to enumerate every field with [`full`](#full).

```yaml
assert:
  body:
    email: +exists          # must be present (any value, including null)
    password_hash: +absent  # must NOT be present
```

`+null` is stricter than `+exists`: the field must be present **and** JSON null.

#### Variable references

An expected value can be a [variable reference](variables.md) (bare or `${}`, inline or whole-value). The response field must equal the resolved value.

```yaml
assert:
  body:
    username: $setup.username
    role: $vars.expected-role
    post_id: $create-post.response.id
    greeting: "Hello, ${setup.username}"
```

#### Regex assertions

Use `re/<pattern>` to assert that a string field matches a regular expression.

```yaml
assert:
  body:
    token: "re/[A-Za-z0-9]{32}"
    slug: "re/[a-z0-9-]+"
    created_at: "re/\\d{4}-\\d{2}-\\d{2}"
```

If the field is not a string, or if it does not match the pattern, the assertion fails with a descriptive message.

#### Size assertions (`len`)

Use `len(field)` as the key to assert the length of a string, array, or object.

```yaml
assert:
  body:
    token: +str
    len(token): 32           # exactly 32 characters
    len(token): '>=8'        # at least 8 characters
    len(token): '<=64'       # at most 64 characters
    items: +arr
    len(items): '>=1'        # at least one item
```

Supported operators: `=`, `>=`, `<=`, `>`, `<`.

#### Numeric value comparisons

An expected value written as a comparison expression checks the field's
**numeric value** (not its length). Same operators as `len(...)`:

```yaml
assert:
  body:
    capacity: '>=1'
    price_cents: '>=0'
    discount: '<100'
```

A comparison-shaped string is always interpreted as a numeric comparison; if the
field is not a number, the assertion fails.

#### Array membership (`+exists`)

To assert that an array contains at least one element matching a partial object,
use `+exists` as a single-key map under the array field. The inner fields use the
full assertion vocabulary (exact value, `+type`, variable references, etc.):

```yaml
assert:
  body:
    posts:
      +exists:
        id: ${new-post.response.post_id}
        title: +str
```

The assertion passes if **some** element matches all of the inner field
assertions. To pull a specific element out by position instead, index it in a
[variable reference](variables.md#array-indexing) (`$list.response.posts.0.id`).

---

### `headers`

Asserts fields within the HTTP response headers. Header names are matched case-insensitively; examples use lowercase to match yapitest's assertion output. Fields not listed are ignored.

Header values support the same exact value, type, variable reference, regex, and `len(...)` assertions as `body`.

```yaml
assert:
  headers:
    content-type: "re/application/json.*"
    x-request-id: +str
    x-api-version: $vars.expected-api-version
    len(x-request-id): 36
```

If a response contains multiple values for the same header, yapitest exposes that header as an array:

```yaml
assert:
  headers:
    set-cookie:
      - "session=abc"
      - "theme=dark"
```

---

### `full`

When `full: true`, every field in the response body must be explicitly listed in `body`. Any field present in the response but missing from the assertion fails the test. `full` does not apply to `headers`; extra response headers are ignored.

```yaml
assert:
  full: true
  body:
    id: +int
    name: +str
    email: +str
    created_at: +str
```

Useful for detecting when an API starts leaking fields it shouldn't (passwords, internal IDs, etc.) or when a response schema changes unexpectedly.

---

### `duration`

Asserts that the HTTP request completed within a time limit. Accepts a bare integer (milliseconds), or a string with a `ms` or `s` suffix.

```yaml
assert:
  duration: 500       # under 500ms
  duration: 500ms     # same
  duration: 2s        # under 2 seconds
```

---

## Complete example

```yaml
# test-posts.yaml

test-create-and-get-post:
  setup: create-user
  steps:
    - id: new-post
      path: /api/post/create
      method: POST
      headers:
        API-Token: $setup.token
      data:
        title: "Hello World"
        body: "My first post"
        ref: "re/REF-[0-9]{6}"
      assert:
        status-code: 201
        duration: 1s
        headers:
          content-type: "re/application/json.*"
        body:
          post_id: +int

    - path: /api/post/$new-post.response.post_id
      assert:
        status-code: 200
        duration: 500ms
        headers:
          content-type: "re/application/json.*"
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
