# Yapitest

Yapitest (Yaml API Testing) is an API testing framework composed entirely of YAML files. Testing is frustrating enough already, and Yapitest aims to simplify the entire process of API testing through a simple interface. Frankly, the entire `expect(value).toBe(0)` stuff keeps me up at night.

### Table of Contents

- [Example](#example)
- [Installation](#installation)
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

To install, run:

```
pip install yapitest
```

Then you can run it via `yapitest` in your terminal.

## Further Documentation

- [Config Files](./Configs.md)
- [Tests](./Tests.md)


