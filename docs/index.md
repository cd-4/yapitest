# Yapitest

Yapitest (Yaml API Testing) is an API testing framework composed entirely of YAML files. Testing is frustrating enough already, and Yapitest aims to simplify the entire process of API testing through a simple interface.

!!! note "Alpha"
    Yapitest is still in alpha and there may be some bugs. Feel free to open a [Pull Request](https://github.com/cd-4/yapitest/pulls) or submit an [issue](https://github.com/cd-4/yapitest/issues) and I will try to get it tested and merged as quickly as possible.

## Installation

```bash
pip install yapitest
```

Then run it via `yapitest` in your terminal:

```bash
yapitest path/to/tests/
```

## Example

The yapitest test format was designed to be as simple as possible. Even if you have never seen a yapitest test before, you can probably infer all of what the test is doing.

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

1. The test name (`test-create-and-get-post`) describes what it does.
2. `setup: create-user` runs a reusable step defined in a config file before the test begins.
3. The first step sends a `POST` to `/api/post/create` with a title, body, and auth token from the setup, then asserts the status code is `201`.
4. The second step sends a `GET` to `/api/post/$create-post.response.post_id` — the `$create-post.response.post_id` value is pulled from the JSON response of the previous step — and asserts the body contains the expected title and body.

## Further Documentation

- [Tests](tests.md) — full reference for the test file format
- [Config Files](config.md) — variables, URLs, and reusable step-sets
