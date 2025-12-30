from typing import Dict, Optional, List
from ..utils.time import get_time_ms

from ..utils.dict_wrapper import DeepDict
from ..test.config import ConfigData
from ..test.step import TestStep
from ..utils.console import *


class Test(DeepDict):

    def __init__(self, name: str, data: Dict, parent_config: Optional[ConfigData]):
        super().__init__(data)
        self.name = name
        self.config = self._get_config(parent_config)
        self.steps = self._get_steps()
        self.status = "pending"
        self.groups = self.data.get("groups", [])

    def in_groups(self, groups: List[str]):
        for group in groups:
            if group in self.groups:
                return True
        return False

    def _get_config(self, parent_config: Optional[ConfigData]) -> Optional[ConfigData]:
        config_data = self.data.get("config", None)
        if config_data is None:
            return parent_config
        return ConfigData(config_data, self.file, parent_config)

    def _get_steps(self):
        steps = []

        setup_name = self.data.get("setup")
        if setup_name is not None:
            setup = self.config.get_step_set(setup_name)
            if setup is None:
                raise Exception(f"Setup {setup_name} not defined")
            setup.id = "setup"
            steps.append(setup)

        for step_data in self.data.get("steps", []):
            new_step = TestStep(step_data, self.config)
            steps.append(new_step)
        return steps

    def _run_cleanup(self):
        cleanup_name = self.data.get("cleanup")
        if cleanup_name is None:
            return
        cleanup = self.config.get_step_set(cleanup_name)
        cleanup.run()

    def run(self):
        print(
            f"{YELLOW}[RUNNING]{RESET} {DIM}{self.name}...{RESET}", end="", flush=True
        )
        start_time = get_time_ms()
        prior_steps = {}
        for step in self.steps:
            step.run(prior_steps)
            if step.id is not None:
                prior_steps[step.id] = step
            if not step.passed:
                self.status = "failed"
                break
        self._run_cleanup()
        end_time = get_time_ms()
        duration_ms = end_time - start_time
        if all([s.passed for s in self.steps]):
            self.status = "passed"

        if self.status == "passed":
            print(f"\r{BOLD_GREEN}[PASS]{RESET} {self.name}      ", flush=True)
        else:
            print(f"\r{BOLD_RED}[FAIL]{RESET} {self.name}      ", flush=True)

        return self.get_results(duration_ms)

    def get_status(self):
        # Result status: "passed", "failed", "skipped", "pending", or "other".
        # TODO: Get result status
        return self.status

    def print_fail_summary(self):
        print(f"{RED}  {self.name}{RESET}")
        for step in self.steps:
            if step.passed or not step.has_run:
                continue
            if step.id:
                print(f"    Step: {step.id}")
            else:
                print(f"    Step: {step.path}")

            for assertion in step.assertions:
                if not assertion.checked:
                    continue
                if assertion.passed:
                    continue
                print(f"      {RED}FAIL: {RESET} {assertion.get_message()}")

    def get_results(self, duration_ms: int):
        output = {
            "name": self.name,
            "status": self.get_status(),
            "duration": duration_ms,
            "type": "API",
            "extra": [s.get_json() for s in self.steps],
        }
        if False:
            output["message"] = "GET MESSAGE"
        return output
