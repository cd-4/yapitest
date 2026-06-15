# Interpolation Engine & Assertion Features — Design

**Date:** 2026-06-15
**Branch:** library

---

## Overview

A batch of seven features for the Rust implementation (`rs/`), delivered as one
spec in three dependency-ordered phases:

1. **Interpolation engine (#1)** — a single inline-interpolation engine reused
   everywhere a string can appear (headers, data values, paths, step-set
   output). Foundational; everything else builds on it.
2. **Assertions** — array indexing + `+exists` membership matcher (#3),
   `+exists`/`+absent`/`+null` presence markers (#5), numeric value comparisons
   (#6), `status-code` lists (#7).
3. **Step mechanics** — parameterized step-sets (#2), `query:` map (#4).

Each feature ships with Rust unit tests and positive + `-fail` API tests against
the sample Flask app in `testing/api`, extending the app where a feature needs
an endpoint to exercise it.

---

## Phase 1 — Unified interpolation engine (#1)

### Problem

Interpolation is currently reimplemented in four places, each whole-value-only:

- `clean_headers` (`test_step.rs`) — now does inline interp, but only the bare form.
- `clean_request_data` (`test_step.rs`) — whole value `$x` or `re/...`.
- `clean_path` (`test_step.rs`) — per `/`-segment, segment must start with `$`.
- step-set `output` resolution (`config.rs:88`) — whole value `$step.field` only.

Inline interpolation like `Authorization: "Bearer $setup.token"` and the step-set
output workaround `auth-header: "Bearer $login.response.access_token"` both fail.

### Reference forms

Both supported:

- **Delimited** `${path}` — everything up to the matching `}` is the key.
  Enables surrounding text and adjacency:
  `"Bearer ${setup.token}"`, `"id=${reg.response.id}-done"`.
- **Bare** `$path` — `$` followed by `[A-Za-z_][A-Za-z0-9_.-]*`. Keeps all
  existing tests working. A lone `$`, or `$` followed by a non-identifier
  (e.g. `$5`), stays literal.
- **Escape** `$$` → a single literal `$`.

`path` is resolved through the existing `get_variable` (config vars, prior-step
responses, recursive `$` chains, `re/` generation) — unchanged.

### Two entry points over one scanner

A single scanner walks the template emitting literal spans and resolved
references. Two public helpers wrap it:

- **`interpolate_string(template, config, prior_steps) -> Result<String>`**
  Always stringifies resolved scalars (string / number / bool); errors on
  object / array / null. Used by **headers** and **path** (HTTP wire values are
  always strings).

- **`resolve_value(template, config, prior_steps) -> Result<Value>`**
  - If the template is *exactly one* reference with no surrounding literal text
    (`"$x.y"` or `"${x.y}"`), preserve the resolved **type** — int stays int,
    object stays object. This keeps current typed-data behavior intact
    (`user_id: $reg.response.id` is sent as a JSON int).
  - If the template mixes literal text and/or multiple references, stringify to
    a `Value::String`.
  - A pure-literal string with no `$` returns `Value::String(template)`.
  - `re/...` (whole value) still generates a string.
  Used by **data values** and **step-set output**.

### Application points

- **Headers** (`clean_headers`): call `interpolate_string` per value (already
  close; gains `${}` support).
- **Data** (`clean_request_data`): string leaf becomes
  `re/` → generate, else → `resolve_value`. Objects/arrays recurse as today.
- **Path** (`clean_path`): run `interpolate_string` over the whole path string.
  `/` and `}` are natural reference boundaries, so segment semantics are
  preserved; leading/trailing slashes are literal text and pass through. Replaces
  the manual per-segment loop.
- **Step-set output** (`config.rs`): replace the bespoke whole-value resolver
  with `resolve_value`, so `"Bearer $login.response.token"` works and typed
  outputs are preserved.

### Placement

The engine depends on `get_variable` and `TestStepResult` (both in
`test_step.rs`) and is consumed by `config.rs`. It lives in `test_step.rs` (or a
new `interp` module) and is called from `config.rs` as
`test_step::resolve_value(...)`. Cross-module references within the single crate
are fine.

### Tests

- Unit: bare ref, `${}` ref, surrounding text, multiple refs, `$$` escape, lone
  `$`/`$5` literal, type preservation vs stringification, non-scalar error,
  `re/` passthrough.
- API: an inline `${}` header test (positive) added alongside the existing
  `test-header-interpolation.yaml`; a data-value inline test.

---

## Phase 2 — Assertions

### #3 Array support

**Indexing in variable paths.** Extend `TestStepResult::get_field` navigation: a
path segment that parses as a `usize` indexes into an array (in addition to the
current object-key lookup). Out-of-range / non-array → `Ok(None)` (treated as
not found). Enables `$list.response.items.0.id` in paths, data, and assertions.

**Membership matcher.** When an expected field value is a **single-key map**
whose key is `+exists`, the matcher activates:

```yaml
body:
  items:
    +exists:
      id: ${new-post.response.post_id}
      title: +str
```

Semantics: observed must be an array; the assertion passes if **at least one**
element matches the inner partial object (one or more field matchers). Element
match reuses the existing object-compare logic in non-`full` mode against a
throwaway assertion buffer — an element matches when all its inner assertions
pass. On failure, one assertion is emitted naming the field. `+exists` follows
the existing `+`-marker convention (`+str`, `+absent`, `+null`), so it does not
collide with real response field names.

### #5 Presence / absence / null

Three independent presence markers, all handled where presence is known
(`compare_data_objects`, before the missing-field error):

- **`+absent`** — passes iff the key is **absent** from the observed object;
  fails (clear message) if present. Mirror of `+exists`.
- **`+exists`** (plain value) — passes iff the key is **present**, regardless of
  its value (including `null`); fails if missing. The positive mirror of
  `+absent`. (`+exists` also serves as the array membership matcher in #3 when
  used as a map key; the two positions are unambiguous — value vs. single-key
  map.)
- **`+null`** (alias `+nil`) — added to the type-check match in
  `compare_primitive_values`: passes iff observed is JSON `null`
  (`ends_at: +null`). Distinct from `+absent` (missing key) and `+exists`
  (present, any value).

```yaml
body:
  email: +exists          # must be present, value irrelevant
  password_hash: +absent  # must NOT be present
  ends_at: +null          # must be present AND JSON null
```

### #6 Numeric value comparisons

An expected **string** that fully matches a comparison expression
(`">=1"`, `"<5"`, `"=0"`, `">0"`, `"<=10"`) is interpreted as a numeric
comparison against the observed **number** (compared as `f64`). Reuses the
operator grammar already used by `len(...)`, extended from `i64` to `f64`.

```yaml
body:
  capacity: ">=1"
  price_cents: ">=0"
```

Detection: in `compare_primitive_values`, before the literal-equality fallback,
test the expected string against the comparison regex; if it matches and the
observed value is a number, evaluate the comparison. If the observed value is not
a number, fail with a clear message. Documented that comparison-shaped strings
are interpreted as comparisons (escape hatch not needed for the listed use
cases).

### #7 status-code list

`check_status_code` already takes a `serde_json::Value`. Extend it: if the value
is an **array**, the check passes if the actual code matches **any** element
(each element evaluated by the existing exact / wildcard logic).

```yaml
assert:
  status-code: [200, 201]
# wildcards allowed in the list too:
  status-code: [200, "4xx"]
```

---

## Phase 3 — Step mechanics

### #2 Parameterized step-sets

`setup` (and inline step-set references in a `steps` list) accept either the
current string form or a map:

```yaml
setup:
  name: login
  args:
    email: alice@example.com
```

```yaml
# inside the `login` step-set:
data:
  email: $args.email
  password: ${args.password}
```

Semantics:

- `args` values are resolved in the **caller's** scope (so
  `args: {token: $vars.x}` works) before injection.
- The resolved args are injected as an `$args.*` namespace visible to the
  step-set's steps and its `output`, implemented by inserting a synthetic
  `args` entry (an `output_data`-backed `TestStepResult`) into the `prior_steps`
  map passed to the step-set's `run`. `$args.email` then resolves via the
  existing prior-step → `get_field` path.
- `setup` deserialization changes from `Option<String>` to an enum accepting a
  bare string or `{name, args}`. The inline step-list reference parser gains the
  same map form.

Collapses near-identical `login-alice` / `login-bob` / `login-carol` step-sets
into a single parameterized `login`.

### #4 Query map

A `query:` map on a step, peer of `headers:` / `data:`:

```yaml
- path: /auth/v1/token
  method: POST
  query:
    grant_type: password
    redirect: ${vars.callback}
  assert:
    status-code: 200
```

Semantics: each value is run through `interpolate_string`, then attached via
reqwest's `.query(&[(k, v)])`, which percent-encodes keys and values. Putting
query params directly in `path:` remains discouraged (encoding undefined);
`query:` is the supported, variable-friendly mechanism. If `path:` already
contains a `?`, the `query:` map entries are appended (reqwest merges).

---

## Cross-cutting

### Sample API extensions (`testing/api/main.py`)

Add endpoints only as needed to exercise features end-to-end:

- A list/feed endpoint returning an array of objects (for #3 indexing +
  `+exists`), e.g. `GET /api/post/list` already exists and returns `posts`.
- An endpoint returning a field that can be `null` vs absent (for #5), e.g. a
  user/profile field.
- An endpoint reading query params (for #4), e.g. `GET /api/user/search?q=...`.

### Tests

Every feature: Rust unit tests + a YAML test file (or additions to an existing
one) in `testing/tests/test_dir` with **positive** tests and **`-fail`** tests
(intentionally-wrong assertions) proving the framework detects failures, per the
existing convention.

### Docs

`Tests.md` and `Configs.md` updated per phase: interpolation forms, `contains`,
`+absent`/`+null`, value comparisons, `status-code` lists, step-set `args`, and
`query:`.

---

## Decisions locked

- **Syntax:** both `${path}` and bare `$path`; `$$` escapes a literal `$`.
- **Type preservation:** data/output whole-value refs preserve type; mixed
  text/refs stringify.
- **Numeric comparisons:** comparison-shaped expected strings are auto-detected
  (no `cmp(...)` marker).
- **Array assertions:** both indexing in paths and a `+exists` membership
  matcher.
- **Presence markers:** `+exists` (present, any value), `+absent` (missing),
  `+null` (present and JSON null). `+exists` doubles as the array membership
  matcher when used as a single-key map.
- **Delivery:** one spec, phased implementation with review checkpoints between
  phases.

---

## Out of scope

- Python and Zig implementations (this branch is Rust-only).
- Header/query value templating beyond simple string interpolation (no
  conditionals/loops).
- Schema-style validation beyond the per-field assertions described.
