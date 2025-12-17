from typing import Dict, Optional, Any
import requests
from utils.dict_wrapper import DeepDict
from utils.exc import RequiredParameterNotDefined


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
        self.assert_data = self.step_data.get("assert")

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

        return base_url + path

    def _get_required_parameter(self, name: str):
        if name not in self.step_data:
            raise RequiredParameterNotDefined(name)
        return self.step_data[name]

    def _get_special_value(self, key: str, prior_steps):
        value = self.config.get(key)
        if value is not None:
            return value

        keys = key.split(".")
        step_id = keys[0][1:]
        if step_id in prior_steps:
            step = prior_steps[step_id]
            output = step._get_keys(keys[1:])
            if output is None:
                raise Exception(f"Parameter `{key}` not defined")
            return output
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
            headers = self.sanitize(self.header_data)
            self.set_value("headers", headers)
            kwargs["headers"] = headers

        if self.request_data is not None:
            data = self.sanitize(self.request_data, prior_steps)
            self.set_value("data", data)
            kwargs["json"] = data

        url = self._get_url(prior_steps)
        response = method(url, **kwargs)

        response_json = response.json()
        self.set_value("response", response_json)

        self.make_assertions(response)

    def make_assertions(self, response):
        pass


class StepSet(DeepDict):

    def __init__(self, data: Dict, config: "ConfigData"):
        self.config = config
        inputs = data.get("inputs", {})

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
            prior_steps.append(step)

        outputs = {}
        for key, value in self.data.get("output", {}).items():
            outputs[key] = self.get(value)

        return outputs, prior_steps


class StepGroupStep(TestStep):

    def __init__(self, step_group: StepSet, config: "ConfigData"):
        self.config = config
        self.step_group = step_group

    def run(self, prior_steps: Dict[str, "TestStep"]):
        outputs, group_prior_steps = self.step_group.run(prior_steps)
        for key, value in outputs.items():
            self.set_value(key, value)
        return prior_steps
