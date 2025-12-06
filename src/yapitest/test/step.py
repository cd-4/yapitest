from typing import Any, Dict, List


class RequiredStepParameterNotDefined(Exception):

    def __init__(self, step, parameter_name: str):

        msg = f"Parameter {parameter_name} not defined in step "
        if step.id is not None:
            msg += f"[{step.id}]"
        else:
            msg += f"[{step.index}]"

        super().__init__(msg)


class TestStep:

    def __init__(self, index: int, data: Dict, config_set):
        self.raw_data = data
        self.config_set = config_set
        self.id = self.raw_data.get("id")
        self.index = index
        self.path = self._get_required_parameter("path")
        self.method = self.raw_data.get("method", "GET").lower()
        self.request_data = self.raw_data.get("data")
        self.header_data = self.raw_data.get("headers")
        self.assertion_data = self.raw_data.get("assert")
        self.url = self._get_url()

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

    def pre_run(self):
        pass

    def post_run(self):
        pass

    def run(self):
        self.pre_run()
        self.run_internal()
        self.post_run()

    def run_internal(self):
        pass


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
