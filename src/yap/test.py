import requests
from typing import Optional, Tuple, Any
from utils.dict_wrapper import flatten_dict


class InvalidTestException:

    def __init__(self, test, message):
        super().__init__(f"Invalid Test: {test} " + message)


class ValueNotFoundInResponseException:

    def __init__(self, value):
        super().__init__(f"Value not found: {value}")


class Assertion:

    def __init__(self, step):
        self.step = step

    def get_message(self) -> str:
        return "ASSERTION METHOD NOT DEFINED"

    def passed(self) -> bool:
        return False

    def get_assertion_string(self) -> str:
        return f"{self.step.get_name()}: {self.get_message()}"

    def __str__(self):
        return self.get_assertion_string()


class BasicAssertion(Assertion):

    def __init__(self, step, actual_value, desired_value, message):
        self.step = step
        self.actual_value = actual_value
        self.desired_value = desired_value
        self.message = message
        super().__init__(step)

    def passed(self) -> bool:
        return self.desired_value == self.actual_value

    def get_message(self) -> str:
        return self.message


class StatusCodeAssertion(BasicAssertion):

    def __init__(self, step, actual_code, desired_code):
        eq = "==" if int(actual_code) == int(desired_code) else "!="
        msg = f"Status code ({actual_code} {eq} {desired_code})"
        super().__init__(step, int(actual_code), int(desired_code), msg)


class BodyAssertion(Assertion):

    def __init__(self, step, key, desired_value, response_body):
        self.key = key
        self.desired_value = desired_value
        self.response_body = response_body
        super().__init__(step)
        passes, message = self.check()
        self.passes = passes
        self.message = message

    def check_type(self, desired_type, value):
        if isinstance(value, desired_type):
            return True, f"`{self.key}` is type {desired_type}"
        else:
            return False, f"`{self.key}` is not type {desired_type}"

    def check(self) -> (bool, str):
        des_str = str(self.desired_value)
        # If key does not exist in response, fail
        if self.key not in self.response_body:
            return False, f"`{self.key}` not in response"

        actual_value = self.response_body[self.key]
        if des_str.startswith("+"):
            if len(des_str) == 1:
                return True, f"`{self.key}` in response"
            else:
                rest = des_str[1:]
                if rest.lower() in ["str", "string"]:
                    return self.check_type(str, actual_value)
                if rest.lower() in ["bool", "boolean"]:
                    return self.check_type(bool, actual_value)
                if rest.lower() in ["arr", "array", "list"]:
                    return self.check_type(list, actual_value)
                if rest.lower() in ["obj", "dict", "dic"]:
                    return self.check_type(dict, actual_value)

        # Fallback to check equality
        matches = actual_value == self.desired_value
        if matches:
            return True, f"`{self.key}` matches {self.desired_value}"
        else:
            return False, f"`{self.key}` {actual_value} != {self.desired_value}"

    def passed(self) -> bool:
        return self.passes

    def get_message(self) -> str:
        return self.message


class ApiTestStep:

    def __init__(self, data, test, index):
        self.raw_data = data
        self.api_test = test
        self.index = index
        # Optional
        self.id = data.get("id")
        # Mandatory
        self.path = data.get("path")
        if self.path is None:
            msg = f"Steps must contain `path` (step id:{self.id})"
            raise InvalidTestException(self.api_test, msg)

        self.url = data.get("url")
        if self.url is None:
            self.url = self.config.urls.get("base")
        if self.url is None:
            msg = f"Steps must contain `url` or it must be defined in yap-config.yaml (step id:{self.id})"
            raise InvalidTestException(self.api_test, msg)
        # Default to GET
        self.method = data.get("method", "GET").lower()
        # If None, don't send any data
        self.data = data.get("data", None)
        # If None, don't send any headers
        self.headers = data.get("headers", {})
        # If None, don't check anything
        self.assertions = data.get("assert", None)

        self.completed_assertions = []
        self.should_exit = False
        self.is_complete = False

    def last_assertion(self) -> Optional[Assertion]:
        if self.completed_assertions:
            return self.completed_assertions[-1]
        return None

    def get_name(self):
        if self.id is not None:
            return f"[{self.index}] {self.id}"
        else:
            return f"[{self.index}] {self.method} {self.path}"

    @property
    def config(self):
        return self.api_test.config

    def sanitize(self, value):
        return self.api_test.sanitize(value)

    def run(self):
        method = getattr(requests, self.method)

        headers = {}
        for header, value in self.headers.items():
            headers[header] = self.sanitize(value)

        data = self.sanitize(self.data)

        response = method(self.url + self.path, json=data, headers=headers)
        self.response_data = response.json()
        self.perform_assertions(response)

    def add_assertion(self, assertion: Assertion) -> None:
        self.completed_assertions.append(assertion)
        if not assertion.passed():
            self.should_exit = True

    def assert_status_code(self, response):
        if "status-code" not in self.assertions:
            return
        desired_status_code = self.assertions["status-code"]
        assertion = StatusCodeAssertion(
            self,
            response.status_code,
            desired_status_code,
        )
        self.add_assertion(assertion)

    def assert_body(self, response):
        if "body" not in self.assertions:
            return

        body_assertions = flatten_dict(self.assertions["body"])
        response_body = flatten_dict(response.json())

        for key in body_assertions:
            desired_val = body_assertions[key]
            if isinstance(desired_val, str) and desired_val.startswith("$"):
                print(desired_val)
                desired_val = self.api_test.get_var_value(desired_val)

            assertion = BodyAssertion(self, key, desired_val, response_body)
            self.add_assertion(assertion)

    def perform_assertions(self, response):
        self.assert_status_code(response)
        if self.should_exit:
            return
        self.assert_body(response)
        if self.should_exit:
            return


class ApiTest:

    def __init__(self, testfile, name, data, config):
        self.name = name
        self.testfile = testfile
        self.data = data
        self.vars = self.data.get("vars", {})
        self.config = config

    def _split_path(self, value) -> Tuple[str, str]:
        ind = value.index(".")
        return value[:ind], value[ind + 1 :]

    def sanitize_value(self, value):
        if not isinstance(value, str) or not value.startswith("$"):
            return value

        key, rest = self._split_path(value[1:])

        # Get variables either from test conig, then global config
        if key == "vars":
            return self.get_var_value(rest)

        step = self.steps_by_id.get(key, None)
        if step is None:
            raise InvalidTestException(self, f"Step ID Not Found (ID: {key})")

        if not step.is_complete:
            raise InvalidTestException(self, "Step references future step")

        keys = rest.split(".")
        if keys[0] == "response":
            jsondata = step.response_data
            for key in keys[1:]:
                if key in jsondata:
                    jsondata = jsondata[key]
                else:
                    print(value)
                    raise ValueNotFoundInResponseException(value)
            return jsondata
        raise ValueNotFoundInResponseException(value)

    def sanitize(self, value) -> Any:
        if isinstance(value, dict):
            output = {}
            for key, val in value.items():
                output[key] = self.sanitize(val)
            return output
        elif isinstance(value, list):
            return [self.sanitize(i) for i in value]
        else:
            return self.sanitize_value(value)

    def get_var_value(self, var_name):

        if var_name in self.vars:
            return self.vars[var_name]
        return self.config.get_variable(var_name)

    def __repr__(self) -> str:
        file_path = self.testfile._get_readable_path()
        return "ApiTest: " + file_path + ":" + self.name

    def __str__(self) -> str:
        return self.__repr__()

    def generate_steps(self) -> None:
        if "steps" not in self.data:
            raise InvalidTestException(test, "`steps` not defined")
        self.test_steps = []
        self.steps_by_id = {}
        for step_data in self.data["steps"]:
            step = ApiTestStep(step_data, self, len(self.test_steps))
            self.steps_by_id[step.id] = step
            self.test_steps.append(step)

    def run(self):
        self.generate_steps()

        for step in self.test_steps:
            self.current_step = step
            print("------------")
            step.run()
            step.is_complete = True
            if step.should_exit:
                print("Test Failed")
                print(str(step.last_assertion()))
                break
