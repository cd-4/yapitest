# Variables

Variables let you pass data between steps, reference config values, and keep tests DRY. Any string value starting with `$` is treated as a variable reference and resolved before the request is made.

Variable references work in:

- Step `path` segments
- Step `url`
- Step `headers` values
- Step `data` values (at any nesting depth)
- Assertion `body` expected values
- Config `urls` values
- Step-set `output` values

---

## Syntax

```
$<namespace>.<key>[.<nested-key>...]
```

Keys are dot-separated and resolve into nested objects. For example, `$login.response.user.id` navigates `response` → `user` → `id` from the step named `login`.

---

## Available namespaces

### `$vars.<name>`

A variable defined in a [config file](config.md#vars) or inline test config.

```yaml
data:
  username: $vars.sample-user
  password: $vars.api-key
```

### `$urls.<name>`

A URL defined in a [config file](config.md#urls).

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

The JSON **request body** of a named step. Useful for referencing data you sent:

```yaml
- id: create-user
  path: /api/user/create
  method: POST
  data:
    username: alice

- path: /api/user/$create-user.data.username    # "alice"
```

### `$setup.<key>`

An output key from the step-set declared as `setup:` on the test. Output keys are defined in the step-set's [`output`](config.md#output) block.

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

When a step-set is referenced inline in `steps`, its outputs are accessible by the step-set's name:

```yaml
steps:
  - create-user
  - path: /api/post/create
    headers:
      API-Token: $create-user.token
```

---

## Resolution order

When a reference like `$foo.bar` is encountered, yapitest resolves it in this order:

1. **Config values** — checks if `foo` is `vars` or `urls` and returns the matching entry
2. **Prior steps** — checks if `foo` matches the `id` of a step that has already run, then navigates the dot-path into its `response` or `data`
3. **Setup/step-set outputs** — checks if `foo` matches `setup` or the name of an inline step-set reference

If none of these match, an error is thrown and the test fails immediately.

---

## Variables in paths

Variables in path segments are resolved per `/`-delimited segment. The resolved value must be a string or integer; other types cause an error.

```yaml
path: /api/user/$setup.username
path: /api/post/$new-post.response.id    # integer — converted to string automatically
```

---

## Variables as entire data payloads

A variable reference can be the entire `data` value to forward a complete object:

```yaml
- id: original
  path: /api/user/create
  method: POST
  data:
    username: alice
    role: admin

- path: /api/user/clone
  method: POST
  data: $original.data      # re-send the entire request body
```

---

## Variables in assertions

Expected values in `body` assertions can be variable references. The actual response field is compared against the resolved value:

```yaml
test-profile-reflects-setup:
  setup: create-user
  steps:
    - path: /api/user
      method: GET
      headers:
        API-Token: $setup.token
      assert:
        body:
          name: $setup.username
```

---

## Regex generation

Prefix any string value with `re/` to generate a random string matching that regular expression. This works in `data` fields and in [config `vars`](config.md#generated-from-a-regex-pattern).

```yaml
data:
  username: "re/[a-z]{8}"           # e.g. "kqmvtjzr"
  reference: "re/REF-[0-9]{6}"     # e.g. "REF-482910"
```

A new value is generated for each step that uses it.
