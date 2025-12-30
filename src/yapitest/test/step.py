from typing import Dict, Optional, Any, List
import requests
from ..utils.dict_wrapper import DeepDict
from ..utils.exc import RequiredParameterNotDefined
from ..test.assertions.assertion import Assertion
from ..test.assertions.status_code_assertion import StatusCodeAssertion
from ..test.assertions.body_assertion import get_body_assertions
from ..test.assertions.full_assertion import FullAssertion


class TestStep(DeepDict):

    def __init__(self, step_data: Dict, config: Optional["ConfigData"]):
        self.step_data = step_data
        if config is None:
            self.config = {}
        else:
            self.config = config
        super().__init__({})

        self.id = self.step_data.get("id")
        self.path = self._get_required_parameter("path")
        self.method = self.step_data.get("method", "GET").lower()
        self.header_data = self.step_data.get("headers")
        self.request_data = self.step_data.get("data")
        self.assert_data = self.step_data.get("assert", {})
        self.has_run = False
        self.passed = True

    def _get_base_url(self, prior_steps: Dict[str, "TestStep"]):
        defined_value = self.step_data.get("url")
        if defined_value is None:
            output = self.config.get("$urls.default")
            if output is None:
                raise Exception("Url not defined")
            return self.sanitize(output, prior_steps)

        return self.sanitize(defined_value, prior_steps)

    def _get_url(self, prior_steps: Dict[str, "TestStep"]):
        base_url = self._get_base_url(prior_steps)
        path = self.path
        if not path.startswith("/"):
            path = f"/{path}"

        if base_url.endswith("/"):
            base_url = base_url[:-1]

        real_path = "/"
        for seg in path.split("/"):
            if not seg:
                continue
            seg = self.sanitize(seg, prior_steps)
            real_path += str(seg) + "/"

        if real_path.endswith("/"):
            real_path = real_path[:-1]
        return base_url + real_path

    def _get_required_parameter(self, name: str):
        if name not in self.step_data:
            raise RequiredParameterNotDefined(name)
        return self.step_data[name]

    def _get_special_value(self, key: str, prior_steps):
        value = self.config.get(key)
        if value is not None:
            return value

        keys = key.split(".")
        first_key = keys[0][1:]
        if first_key in prior_steps:
            step = prior_steps[first_key]
            output = step._get_keys(keys[1:])
            if output is None:
                raise Exception(f"Parameter `{key}` not defined")
            return output

        # if first_key == "setup":
        #     raise Exception("SETUP")

        raise Exception(f"Parameter `{key}` not defined")

    def sanitize(self, data: Any, prior_steps: Dict[str, "TestStep"]) -> Any:
        # Sanitize Dict
        if isinstance(data, dict):
            output = {}
            for key, value in data.items():
                output[key] = self.sanitize(value, prior_steps)
            return output

        # Sanitize Lists
        if isinstance(data, list):
            return [self.sanitize(x, prior_steps) for x in data]

        if isinstance(data, str) and data.startswith("$"):
            return self._get_special_value(data, prior_steps)
        return data

    def run(self, prior_steps: Optional[Dict[str, "TestStep"]] = None):
        if prior_steps is None:
            prior_steps = {}

        method = getattr(requests, self.method)

        kwargs = {}

        if self.header_data is not None:
            headers = self.sanitize(self.header_data, prior_steps)
            self.set_value("headers", headers)
            kwargs["headers"] = headers

        if self.request_data is not None:
            data = self.sanitize(self.request_data, prior_steps)
            self.set_value("data", data)
            kwargs["json"] = data

        url = self._get_url(prior_steps)
        self.used_url = url
        self.response = method(url, **kwargs)

        try:
            response_json = self.response.json()
            self.set_value("response", response_json)
        except:
            pass

        self.make_assertions(prior_steps)
        self.has_run = True

    def _get_assertions(self, prior_steps: Dict[str, "TestStep"]) -> List[Assertion]:
        assertions = []

        if self.assert_data.get("full", False):
            assertion = FullAssertion(
                self.response.json(), self.assert_data.get("body", {})
            )
            assertions.append(assertion)

        if "status-code" in self.assert_data:
            desired_status_code = self.assert_data.get("status-code")
            assertion = StatusCodeAssertion(self.response, desired_status_code)
            assertions.append(assertion)

        if "body" in self.assert_data:
            desired_body_data = self.assert_data.get("body")
            response_data = self.response.json()
            assertions.extend(
                get_body_assertions(
                    response_data,
                    desired_body_data,
                    self,
                    prior_steps,
                )
            )

        return assertions

    def make_assertions(self, prior_steps: Dict[str, "TestStep"]):
        self.assertions = self._get_assertions(prior_steps)
        for assertion in self.assertions:
            if not assertion.check():
                self.passed = False

    def get_json(self):
        path = self.path
        if "url" in self.data:
            path = self.used_url + "/" + self.path

        status = "NA"
        if not self.has_run:
            status = "skipped"
        elif self.passed:
            status = "passed"
        else:
            status = "failed"

        json_output = {
            "step": f"{self.method} {path}",
            "status": status,
            "assertions": [a.get_json() for a in self.assertions],
        }
        return json_output


class StepSet(DeepDict):

    def __init__(self, data: Dict, name: str, config: "ConfigData"):
        self.config = config
        inputs = data.get("inputs", {})
        self.once = data.get("once", False)
        self.name = name
        self.has_run = False
        self.passed = True

        new_steps = []
        for step in data.get("steps", []):

            # Use reusable step set
            if isinstance(step, str):
                step_group = self.config.get(step)
                step_group_step = StepGroupStep(step_group, self.config)
                new_steps.append(step_group_step)

            else:
                new_step = TestStep(step, self.config)
                step_id = new_step.id
                if step_id is not None:
                    data[step_id] = new_step
                new_steps.append(new_step)

        self.steps = new_steps
        super().__init__(data)

    def run(self, prior_steps: Dict[str, "TestStep"]):
        for step in self.steps:
            step.run(prior_steps)
            if step.id is not None:
                prior_steps[step.id] = step
            if not step.passed:
                self.passed = False
                break

        outputs = {}
        for key, value in self.data.get("output", {}).items():
            output_value = self.get(value)
            if output_value is None:
                output_value = self.config.get(value)
            outputs[key] = output_value

        self.has_run = True

        return outputs, prior_steps

    def get_json(self):
        status = "NA"
        if not self.has_run:
            status = "skipped"
        elif self.passed:
            status = "passed"
        else:
            status = "failed"
        return {
            "step": f"Step Group: {self.name}",
            "status": status,
        }


class StepGroupStep(TestStep):

    def __init__(self, step_group: StepSet, config: "ConfigData"):
        self.data = {}
        self.config = config
        self.step_group = step_group

    @property
    def passed(self):
        return self.step_group.passed

    def run(self, prior_steps: Dict[str, "TestStep"]):
        if self.step_group.once and self.step_group.has_run:
            return prior_steps

        outputs, group_prior_steps = self.step_group.run(prior_steps)

        for key, value in outputs.items():
            self.set_value(key, value)

        self.has_run = True
        return prior_steps

    def get_json(self):
        return self.step_group.get_json()
