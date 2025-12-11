# Config File

Configurations can store shared variables, as well as test setups and cleanups that can be shared across many tests. They can be defined in their own files called `yapitest-config.yaml` or `config.yaml`. Inside of the dedicated config files, all of the parameters can be defined at the top level. Inside of tests, the configurations must be specified inside of the top level `config` key.

During a test, values from a config will be loaded from config files. By default, only config files from the test directory or parent directories will be applied. For example:

```
yapitest-config.yaml
  - test_dir/
    - config.yaml
    - test_something.yaml
  - another_test_dir/
    - config.yaml
    - another-test.yaml
```

WHen running `test_something.yaml`, only `yapitest-config.yaml` and `test_dir/config.yaml` will be applied to that test. When searching for config values, if there are conflicts, the config value closest to the test wil be applied. First the config in the test file itself will be tried, then the config in the directory, then it will iterate up the directories to find the value. If the value specified in the test is not found, an error will be thrown.


### Variables

Variables can be defined in configs and be accessed by using `$vars.var-name` within tests.

An example config can be found below

```yaml

# yapitest-config.yaml

# Variables to be used in data or assertions
variables:

  # Values can be specified directly if they do not change
  some-value: another-thing

  # Values can also be set from environment variables
  default_url:
    env: BASE_URL
    # If environment variable not found, defaults can be set
    # If default value is not set and env var is not found, an error will be thrown
    default: http://127.0.0.1:8181

```

### Urls

URLs can also be specified inside of config files.

```yaml
urls:
  default: $vars.default_url.
```

In the above example, we are using the variable `default_url` to specify the url with the name `default`.

By default, the url specified with the key `default` will be used as the URL for all API tests unless otherwise specified.

### Steps

Test steps can also be defined within the configs. These are sets of steps that can be re-used across multiple tests to reduce duplication.

Below is the config for the steps. The layout is the same as the steps defined within tests, and more details can be found in [./Tests.md](./Tests.md).

```yaml
step-sets:
  create-user:
    # if `once` is set to true, this step will only be run once per yapitest run. If it is not defined, it will be run each time the step is referenced
    once: false
    steps:
      - id: create-user
        path: /api/user/create
        method: POST
        data:
          username: test-user
          password: test-password
        assert:
          status-code: 200
          body:
            token: +str
      - id: create-post
        path: /api/post/create
        method: POST
        data:
          content: |
            This is the content for the post
        assert:
          status-code: 200
```

These steps could be used in a test like so:

```yaml

test-one:
  steps:
    - id: health-check
      path: /api/healthz
      assert:
        status-code: 200
    # Run the steps in the `create-user` section in the middle of a test
    - create-user
    - id: health-check
      path: /api/healthz
      assert:
        status-code: 200
```

Additionally, there are default `setup` and `cleanup` sections that can be defined in the test like so. When using `cleanup`, the `cleanup` will always run, even if the test fails. However, if the `setup` does not pass, then the `cleanup` will not run.

```yaml

test-one:
  setup: create-user      # references the steps in `create-user`
  cleanup: delete-user    # references the steps in `delete-user`
  steps:
    - id: health-check
      path: /api/user/$create-user.data.username
      assert:
        status-code: 200
```
