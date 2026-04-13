# Yapitest

Yapitest (Yaml API Testing) is an API testing framework where tests are defined entirely in YAML files. No test runners, no assertion libraries, no boilerplate — just declare what you want to call and what you expect back. Frankly, the entire `expect(value).toBe(0)` stuff keeps me up at night.

The Rust implementation is the primary focus of active development and is the recommended way to use Yapitest.

### Table of Contents

- [Example](#example)
- [Installation](#installation)
- [Usage](#usage)
- [Further Documentation](#further-documentation)
  - [Config Files](./Configs.md)
  - [Tests](./Tests.md)



#### Disclaimer

Yapitest is still in alpha and there may be some bugs. Feel free to open up a Pull Request or submit an issue and I will try to get it tested and merged as quickly as possible.


## Example

The yapitest test format was designed to be as simple as possible. Even if you have never seen a yapitest test before, you can probably infer all of what the test is doing.

Here is an example:

```yaml
test-create-and-get-post:
  setup: create-user
  steps:
    - path: /api/post/create
      id: create-post
      method: POST
      headers:
        API-Token: $setup.token
      data:
        title: Some Title
        body: Some message
      assert:
        status-code: 201
    - path: /api/post/$create-post.response.post_id
      assert:
        body:
          title: "Some Title"
          body: "Some message"

```

1. Firstly, the name of the test (`test-create-and-get-post`) gives some indication of what the test is doing.

2. There is a `setup` which is something that runs before the test. This one, `create-user` is reusable step that creates a user. It is defined in a config file. Similarly, you can specify a `cleanup` which is run after the other steps run.

3. Then we have the `steps` section, which includes the actual steps of the tests.

4. The first step sends a `POST` request to `/api/post/create`, with a `title` and `body` in the payload, utilizing an API Token that the `setup` generated. Then asserts that the status code is `201`.

5. The last step of the test sends a `GET` request to `/api/post/$create-post.response.post_id`. This `$create-post.response.post_id` referse to the json response of the `$create-post` step which has a key `post_id` in it. Then inside of the `assert` block we ensure that the data in the body contains the proper `title` and `body` values.

For more information about how to format your tests, please refer to [Tests.md](./Tests.md).


## Installation

Install using [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html):

```bash
cargo install yapitest
```

Then you can run it via `yapitest` in your terminal.

## Usage

Point `yapitest` at one or more directories or YAML files:

```bash
yapitest ./tests
yapitest ./tests/test-users.yaml ./tests/test-posts.yaml
```

**Filtering**

```bash
yapitest ./tests -g auth          # only tests tagged with the "auth" group
yapitest ./tests -i login         # only tests whose name contains "login"
yapitest ./tests -x slow          # exclude tests whose name contains "slow"
```

Flags can be repeated to filter by multiple values:

```bash
yapitest ./tests -g auth -g admin
```

**Parallelism**

By default, tests run on a single thread. Use `-t` to run tests across multiple threads:

```bash
yapitest ./tests -t 4
```

**Verbosity**

Control how much output is printed:

```bash
yapitest ./tests -v 0   # silent — no output
yapitest ./tests -v 1   # test names only
yapitest ./tests -v 2   # default — names + pass/fail (default)
yapitest ./tests -v 3   # full assertion detail
```

**CTRF Report**

Write a [CTRF](https://ctrf.io) JSON report to a file:

```bash
yapitest ./tests --output results.json
```

## Further Documentation

- [Config Files](./Configs.md)
- [Tests](./Tests.md)


