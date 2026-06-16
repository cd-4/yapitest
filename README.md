<p align="center">
  <img src="./YapitestLogo.png" alt="Yapitest" width="300">
</p>

[![Crates.io Version](https://img.shields.io/crates/v/yapitest)](https://crates.io/crates/yapitest)
[![Crates.io Downloads](https://img.shields.io/crates/d/yapitest)](https://crates.io/crates/yapitest)
[![License: MIT](https://img.shields.io/crates/l/yapitest)](LICENSE)
[![Docs](https://img.shields.io/badge/docs-8A2BE2)](https://cd-4.github.io/yapitest)

# yapitest

> API testing without the boilerplate — define your tests in YAML, not code.

Yapitest runs HTTP test sequences from plain YAML files. No test runners, no assertion libraries, no `expect(value).toBe(0)`. Declare what to call, what to send, and what to expect back — yapitest handles the rest.

## Example

```yaml
test-create-and-get-post:
  setup: create-user
  steps:
    - id: create-post
      path: /api/post/create
      method: POST
      headers:
        API-Token: $setup.token
      data:
        title: Some Title
        body: Some message
      assert:
        status-code: 201
        headers:
          content-type: "re/application/json.*"

    - path: /api/post/$create-post.response.post_id
      assert:
        headers:
          content-type: "re/application/json.*"
        body:
          title: Some Title
          body: Some message
```

`setup` runs a reusable step-set (defined in a config file) before the test begins. `$setup.token` and `$create-post.response.post_id` reference values from earlier steps — no glue code required.

## Features

- **Multi-step flows** with `$variable` references (bare or `${...}`) to earlier responses, request bodies, and setup data — inline anywhere a string appears.
- **Reusable step-sets** for shared setup/teardown, optionally parameterized with `args` (`$args.<key>`).
- **Rich assertions** — status codes (exact, wildcard, or list), typed fields (`+str`, `+int`, …), presence/absence/null (`+exists`, `+absent`, `+null`), numeric comparisons (`>=1`), regex, length, array membership (`+exists`) and indexing, response headers, and request duration.
- **Hierarchical config** — base URLs and variables (literal, env-var, or regex-generated) that chain from parent directories.
- **Auto-encoded query strings**, per-step `wait`/`retry`, parallel execution, group/name filtering, and CTRF reports.

Full documentation: **[cd-4.github.io/yapitest](https://cd-4.github.io/yapitest)**.

## Installation

Install with [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html):

```bash
cargo install yapitest
```

## Usage

Point `yapitest` at a directory or one or more YAML files:

```bash
yapitest ./tests
yapitest ./tests/test-users.yaml ./tests/test-posts.yaml
```

### Filtering

```bash
yapitest ./tests -g auth        # only tests in the "auth" group
yapitest ./tests -i login       # only tests whose name contains "login"
yapitest ./tests -x slow        # exclude tests whose name contains "slow"
yapitest ./tests -k "login user"  # only the test named exactly "login user"
```

Flags can be repeated to filter by multiple values:

```bash
yapitest ./tests -g auth -g admin
yapitest ./tests -k "login user" -k "create post"
```

### Parallelism

By default, yapitest picks an appropriate thread count automatically. Override with `-t`:

```bash
yapitest ./tests -t 4
```

### Verbosity

```bash
yapitest ./tests -v 0   # silent
yapitest ./tests -v 1   # test names only
yapitest ./tests -v 2   # names + pass/fail (default)
yapitest ./tests -v 3   # full assertion detail
```

### CTRF Report

Write a [CTRF](https://ctrf.io) JSON report to a file:

```bash
yapitest ./tests --output results.json
```

## Documentation

Full documentation is available at **[cd-4.github.io/yapitest](https://cd-4.github.io/yapitest)**.

- [Writing Tests](./Tests.md)
- [Config Files](./Configs.md)
