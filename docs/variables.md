# Variables

Variables let you pass data between steps, reference config values, and keep tests DRY. A variable reference is resolved before the request is made.

Variable references work everywhere a string can appear:

- Step `path`
- Step `url`
- Step `headers` values
- Step `query` values
- Step `data` values (at any nesting depth)
- Assertion `body` and `headers` expected values
- Config `urls` values
- Step-set `output` values

---

## Syntax

A reference is written in one of two interchangeable forms:

```
$<namespace>.<key>[.<nested-key>...]      # bare
${<namespace>.<key>[.<nested-key>...]}    # delimited
```

Keys are dot-separated and resolve into nested objects. For example,
`$login.response.user.id` navigates `response` → `user` → `id` from the step
named `login`.

### Inline references

References may appear **inline**, surrounded by literal text, and more than once
in a single value:

```yaml
headers:
  Authorization: "Bearer ${setup.token}"      # delimited, with a literal prefix
  X-Trace: "$vars.region-$run.response.id"     # two bare refs plus literal text
```

Use the delimited `${...}` form when a reference is immediately followed by text
that would otherwise be read as part of the key:

```yaml
data:
  slug: "${vars.name}-final"     # without braces, "-final" would join the key
```

- A bare `$` not followed by a valid identifier (a letter or `_`, then
  letters/digits/`_`/`.`/`-`) is left literal — `"$5 off"` stays `"$5 off"`.
- Write `$$` to emit a single literal `$`.

### Type preservation

When a `data` or step-set `output` value is **exactly one** reference with no
surrounding text, the resolved value keeps its JSON type:

```yaml
data:
  user_id: $reg.response.id      # stays an integer
  active: ${vars.is_active}      # stays a boolean
  profile: $reg.response.user    # stays an object
```

When a reference is combined with literal text or other references, the result
is a string (`"id=$reg.response.id"` → `"id=42"`). Header, `path`, and `query`
values are always strings; a reference there that resolves to an object or array
raises an error.

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

### `$args.<key>`

Inside a [parameterized step-set](config.md#parameterized-step-sets-args), the
arguments passed by the caller are available as `$args.<key>`. This namespace
only exists within the step-set's own steps and `output`.

```yaml
step-sets:
  login:
    steps:
      - path: /api/user/login
        method: POST
        data:
          username: $args.username
          password: $args.password
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

Path references resolve inline; the bare and `${}` forms both work, and a
reference may sit next to literal path text. The resolved value is converted to a
string (numbers and booleans are stringified); a reference that resolves to an
object or array raises an error.

```yaml
path: /api/user/$setup.username
path: /api/post/$new-post.response.id    # integer — stringified automatically
path: /api/v${vars.api-version}/status   # ${} so the digit doesn't join the key
```

---

## Array indexing

A numeric path segment indexes into an array. This works anywhere a reference is
navigated — paths, data, assertion-expected values, and step-set output:

```yaml
- id: list
  path: /api/post/list           # response: {"posts": [{"id": 7}, ...]}

- path: /api/post/$list.response.posts.0.id   # → /api/post/7
  assert:
    body:
      id: $list.response.posts.0.id            # the same indexed value
```

Out-of-range indices resolve to "not found" (the same as a missing field).

> To assert that *some* element of an array matches without knowing its position,
> use the [`+exists` membership matcher](tests.md#array-membership-exists) instead.

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
