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
        for step_data in self.data.get("steps", []):
            new_step = TestStep(step_data, self.config)
            steps.append(new_step)
        return steps

    def run(self):
        steps = self._get_steps()
        prior_steps = {}
        for step in steps:
            step.run(prior_steps)
            if step.id is not None:
                prior_steps[step.id] = step
