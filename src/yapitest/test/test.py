from pathlib import Path
from typing import Dict, List
from test.config import ConfigData, ConfigSet
from test.step import TestStep, ReusableStepGroup


class Test:

    def __init__(self, name: str, data: Dict, file: Path, configs: List[ConfigData]):
        self.name = name
        self.raw_data = data
        self.groups = self.raw_data.get("groups", [])

        test_config = self.raw_data.get("config", None)
        if test_config is not None:
            configs.append(ConfigData(self.file, test_config))
        self.config_set = ConfigSet(configs)
        self.steps_by_id = {}

        self.setups = self._get_step_sets("setup")
        self.cleanups = self._get_step_sets("cleanup")

    def _get_step_sets(self, data_key: str) -> List[ReusableStepGroup]:
        setup_data = self.raw_data.get(data_key)
        if not isinstance(setup_data, list):
            setup_data = [setup_data]

        setups = [self.config_set.get_step_set(ss) for ss in setup_data]
        return setups

    def get_steps(self):
        steps_data = self.raw_data.get("steps", [])
        steps = []
        for sd in steps_data:
            new_step = TestStep(len(steps) + 1, sd, self)
            steps.append(new_step)
        return steps

    def run(self):
        for setup in self.setups:
            setup.run()

        steps = self.get_steps()
        for step in steps:
            step.run()
            if step.id is not None:
                self.steps_by_id[step.id] = step

        for cleanup in self.cleanups:
            cleanup.run()
