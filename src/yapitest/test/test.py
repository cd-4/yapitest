from typing import Dict, Optional

from utils.dict_wrapper import DeepDict
from test.config import ConfigData
from test.step import TestStep


class Test(DeepDict):

    def __init__(self, name: str, data: Dict, parent_config: Optional[ConfigData]):
        super().__init__(data)
        self.name = name
        self.config = self._get_config(parent_config)

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
        steps = self._get_steps()
        prior_steps = {}
        for step in steps:
            step.run(prior_steps)
            if step.id is not None:
                prior_steps[step.id] = step

        self._run_cleanup()
