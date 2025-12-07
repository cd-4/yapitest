from typing import Any, Dict, List, Tuple
import posixpath
import requests
from utils.dict_utils import get_dict_depth


class ValueNotFoundInPriorStepException(Exception):

    def __init__(self, step, parameter_name: str):
        msg = f"Value {parameter_name} not found in previous step(s)"
        super().__init__(msg)


class RequiredStepParameterNotDefined(Exception):

    def __init__(self, step, parameter_name: str):

        msg = f"Parameter {parameter_name} not defined in step "
        if step.id is not None:
            msg += f"[{step.id}]"
        else:
            msg += f"[{step.index}]"

        super().__init__(msg)


class TestStep:

    def __init__(self, index: int, data: Dict, test):
        self.raw_data = data
        self.test = test
        self.config_set = test.config_set
        self.id = self.raw_data.get("id")
        self.index = index
        self.path = self._get_required_parameter("path")
        self.method = self.raw_data.get("method", "GET").lower()
        self.request_data = self.raw_data.get("data")
        self.header_data = self.raw_data.get("headers")
        self.assertion_data = self.raw_data.get("assert")
        self.url = self._get_url()
        self.response_data = None

    def _get_url(self) -> str:
        defined_value = self.raw_data.get("url")

        # Get default URL
        if defined_value is None:
            return self.config_set.get_url("default")

        if defined_value.startswith("$urls."):
            url_name = defined_value[len("$urls.") :]
            return self.config_set.get_url(url_name)

        if defined_value.starstwith("$vars."):
            var_name = defined_value[len("$vars.") :]
            return self.config_set.vet_variable(var_name)

    def _get_required_parameter(self, name: str) -> Any:
        if name not in self.raw_data:
            raise RequiredStepParameterNotDefined(self, name)
        return self.raw_data[name]

    def _split_path(self, value) -> Tuple[str, str]:
        ind = value.index(".")
        return value[:ind], value[ind + 1 :]

    def _sanitize_value(self, value: Any) -> Any:
        """
        Intended for disambiguating individual variable values
        """
        if value is None:
            return value

        if not isinstance(value, str) or not value.startswith("$"):
            return value

        key, rest = self._split_path(value)
        if key == "vars":
            return self.config_set.get_variable(rest)
        if key == "urls":
            return self.config_set.get_url(rest)

        rest = rest.split(".")
        step = self.test.steps_by_id.get(key)
        if step is None:
            raise ValueNotFoundInPriorStepException(self, value)

        if rest[0] == "response":
            return get_dict_depth(step.response_data, rest[1:])

    def _sanitize(self, value: Any) -> Any:
        if isinstance(value, dict):
            output = {}
            for key, val in value.items():
                output[key] = self._sanitize(val)
            return output
        elif isinstance(value, list):
            return [self._sanitize(v) for v in value]
        else:
            return self._sanitize_value(value)

    def pre_run(self):
        pass

    def post_run(self):
        pass

    def run(self):
        self.pre_run()
        self.run_internal()
        self.post_run()

    def run_internal(self):
        method = getattr(requests, self.method)

        url = posixpath.join(self.url, self.path)

        kwargs = {}
        if self.request_data is not None:
            kwargs["json"] = self._sanitize(self.request_data)

        if self.header_data is not None:
            kwargs["headers"] = self._sanitize(self.header_data)

        print("Hitting Endpoint: " + url)
        respose = method(url, **kwargs)


class ReusableStepGroup:

    def __init__(self, steps: List[TestStep], once=False):
        self.run_once = once
        self.steps = steps
        self.has_run = False

    def run(self):

        if self.run_once and self.has_run:
            return

        for step in self.steps:
            step.run()
