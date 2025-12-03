# Tests

Tests in Yapitest are designed to be easy to understand and implement. Hopefully you can intuitively understand the tests.

Here is an example:

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
      #headers:
      #  API-Token: $create-user.response.token
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

### `groups`

Tests can have `groups` defined which can be used to select what tests are run by using the `-g/--group` flags. This is useful when you have many tests that need to be run in certain contexts. For example, you may have tests that simulat the purchasing of products that are safe to run in the staging environment but not in production, for this reason, you could tag the `group` with a `staging` tag for CI runs. Then when you run in production, the tests tagged with `production` and **not** `staging` would run keeping your production environment safe.

### `config`

Test can also define a `config` section, which matches the same structure as the [Config.md](./Configs.md). This will take priority over other configs.



## Test Steps

The most important section of a test is the `steps` section, which defines all of the API requests that will be sent to the API in question. Below are the fields that can be used within a test step.

### `id` [optional]

The `id` field can be used if you wish to re-use data from that step in other steps in the test. The properties offered by the step are `data`, which contains the data sent in the API request, and `response` which contains the data received in the API response. If my `id` was set to `create-user` and the API response sent back:

```
{
  "user": {
    "username": "Some_Username",
    "token": "Some_Token"
  }
}
```

I could use `$create-user.response.user.username` to get re-use that username value.
Similarly, if I had posted that above data, I could use `$create-user.data.user.username` to retrieve the username.

### `url` [optional]

The `url` field can be used to specify which base URL you'd like to test. If not set, the URL used will be whatever `urls.base` is set to in the nearest config. If not set in any config file, an error will be thrown.

### `path` [required]

The `path` field will be used to specify the path that will be used for the API request. For example, if my `url` is `localhost:8080` and my `path` is `/api/healthz`, the API will send the request to `localhost:8080/api/healthz`.

### `data` [optional]

The `data` field will be what JSON data is sent to the API. If my `data` block looks like this:

```yaml
...
  data:
    username: SomeUsername
    password: MyP455W0rd
    another_field:
      number: 10
      name: 'Jerry'
...
```

Then the JSON sent to the API endpoint will be this:

```json
{
  "username": "SomeUsername",
  "password": "MyP455W0rd",
  "another_field": {
    "number": 10,
    "name": "Jerry"
  }
}

```

Additionally, variables from config files can be used in the `data` block as well. See below for more details.

##### Reusing Other JSON

If you have some data stored in a variable or some other step's output, you can post the entirety of the json by putting it directly after the `data` key, like this:

```yaml
...
  data: $vars.some-key
...
# Or
...
  data: $some-step-id.response.some-value
...
```


### `assert` [optional]

The `assert` section of the test step can be used to make assertions about the response. An example assertion can look like this:

```yaml
...
  assert:
    status-code: 200
    body:
      data:
        username: "SomeUsername"
        bio: $vars.sample-bio
...
```

This assertion block will assert that the status code of the request is 200, and that the response JSON `data.username` and `data.bio` have the correct values.

Non-specified fields included in the JSON will be ignored. If you wish to assert that every field has been accounted for within the `assert` block, to ensure nothing extra is being returned, you can set `full: true` within the `assert` block.

#### Complex Assertions

You can do more complex assertions like so:

##### Status Codes

You can also use something like `4xx` to assert the return code is in from 400-499. This can be done with other ranges as well like `20x`.

##### Field Exists

If you have a field that you just want to ensure exists with a certain type, you can do something like this:

```yaml
...
  assert:
    body:
      token: +str
...
```

This will assert that the token value exists and is a `String`. Other types can be used as well:

| Type | Assertion |
| --- | --- |
| Boolean | `+bool` |
| String | `+str` |
| Integer | `+int` |
| Float | `+float` |
| Array | `+arr` |
| Dictionary | `+dict` |

##### Field Sizes

If you have a string, list, or dictionary, you can also assert that the length of it is a certain size using something like this:

```yaml
...
  assert:
    body:
      len(token): 20
...
```








