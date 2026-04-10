# Tests

Tests in Yapitest are designed to be easy to understand and implement. Here is a complete example:

```yaml
# test-basic.yaml

test-create-user:
  groups:
    - pull-request
    - prod-safe
  config:
    vars:
      example-password: ASDF123
  steps:
    - id: create-user
      path: /api/user/create
      method: POST
      data:
        username: $vars.sample-user
        password: $vars.example-password
      assert:
        status-code: 200
        body:
          token: +str

    - id: check-token
      path: /api/token/check
      method: POST
      data:
        token: $create-user.response.token
      assert:
        status-code: 200
        body:
          success: true

    - id: check-bad-token
      path: /api/token/check
      method: POST
      data:
        token: asdf234
      assert:
        status-code: 200
        body:
          success: false

test-something-else:
  ...
```

## `groups`

Tests can have `groups` defined, selectable with the `-g`/`--group` flags:

```bash
yapitest tests/ -g pull-request
```

This is useful for separating tests by environment. For example, you might tag tests that modify state with `staging` so they don't run against production.

## `config`

A test can define an inline `config` section, which matches the structure of a [Config File](config.md). Inline config takes priority over any external config files.

---

## Test Steps

The `steps` section defines the API requests sent during the test.

### `id` *(optional)*

Assign an ID to a step to reference its data in later steps. Use `$<id>.response.<field>` to access the response or `$<id>.data.<field>` to access the request body.

```yaml
- id: create-user
  path: /api/user/create
  method: POST
  data:
    username: SomeUser

# Later in the same test:
- path: /api/user/$create-user.response.id
  method: GET
```

### `path` *(required)*

The path appended to the base URL. If `urls.base` is `http://localhost:8080` and `path` is `/api/healthz`, the request goes to `http://localhost:8080/api/healthz`.

Path segments can reference variables:

```yaml
- path: /api/user/$setup.username
```

### `url` *(optional)*

Override the base URL for this step. Can be a literal URL or a `$urls.<name>` reference. If omitted, `urls.base` from the nearest config is used.

### `method` *(optional)*

HTTP method. Defaults to `GET` if not specified.

### `data` *(optional)*

JSON body sent with the request. Values can reference variables:

```yaml
data:
  username: $vars.sample-user
  password: MyP455W0rd
  profile:
    bio: Hello world
```

You can also pass an entire variable as the body:

```yaml
data: $vars.some-key
# or
data: $some-step-id.response.some-value
```

### `headers` *(optional)*

HTTP headers to include. Values can reference variables:

```yaml
headers:
  API-Token: $setup.token
  Content-Type: application/json
```

### `assert` *(optional)*

Assertions to run against the response.

#### `status-code`

Assert the HTTP status code. Supports wildcards:

```yaml
assert:
  status-code: 200    # exact
  status-code: 4xx    # any 400–499
  status-code: 20x    # 200–209
```

#### `body`

Assert fields within the JSON response body. Unspecified fields are ignored unless `full: true` is set.

```yaml
assert:
  body:
    username: "SomeUser"
    id: +int
```

#### `full`

When `full: true`, every field in the response must be accounted for in the `body` assertion. Any extra fields in the response will fail the test.

```yaml
assert:
  full: true
  body:
    id: +int
    name: +str
    email: +str
```

#### Type assertions

Assert a field exists with a specific type using the `+type` syntax:

| Assertion | Matches |
|-----------|---------|
| `+str`    | String  |
| `+int`    | Integer |
| `+float`  | Float   |
| `+bool`   | Boolean |
| `+arr`    | Array   |
| `+dict`   | Object / dictionary |

#### Size assertions

Assert the length of a string, array, or object with `len(<field>)`:

```yaml
assert:
  body:
    token: +str
    len(token): 20        # exactly 20 characters
    len(token): '<=20'    # at most 20
    items: +arr
    len(items): '>=1'     # at least 1 item
```

---

## Inline step-set references

Steps defined in a [config step-set](config.md#step-sets) can be referenced by name directly in a test's `steps` list:

```yaml
test-one:
  steps:
    - id: health-check
      path: /api/healthz
      assert:
        status-code: 200
    - create-user          # runs the `create-user` step-set here
    - path: /api/user
      headers:
        API-Token: $create-user.token
      assert:
        status-code: 200
```

## `setup` and `teardown`

`setup` runs the named step-set before any steps. `teardown` runs after all steps complete (even on failure, if setup succeeded).

```yaml
test-one:
  setup: create-user
  teardown: delete-user
  steps:
    - path: /api/user/$setup.username
      assert:
        status-code: 200
```

Within steps, use `$setup.<field>` to reference setup outputs.
