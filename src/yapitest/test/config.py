from typing import Dict, Optional, List, Any
import os
from pathlib import Path
from ..utils.dict_wrapper import DeepDict
from ..utils.yaml import YamlFile
from ..test.step import StepSet, StepGroupStep


class ConfigData(DeepDict):

    def __init__(self, data: Dict, file: Path, parent: Optional["ConfigData"] = None):
        super().__init__(data)
        self.file = file
        self.parent = parent
        self.step_sets = self._make_step_sets()

    def get_step_set(self, step_set_id: str) -> Optional["TestStep"]:
        step_set = self.step_sets.get(step_set_id, None)
        if step_set is not None:
            return step_set

        if self.parent is not None:
            return self.parent.get_step_set(step_set_id)

        return None

    def _make_step_sets(self):
        if "step-sets" not in self.data:
            return

        step_sets = {}

        step_sets_data = self.data["step-sets"]
        for set_key, set_data in step_sets_data.items():
            step_set = StepSet(set_data, set_key, self)
            step_group_step = StepGroupStep(step_set, self)
            step_sets[set_key] = step_group_step
        return step_sets

    def set_parent(self, parent: "ConfigData") -> None:
        self.parent = parent

    def _get_keys(self, keys: List[str]) -> Optional[Any]:
        value = super()._get_keys(keys)
        if value is None and self.parent is not None:
            return self.parent._get_keys(keys)

        if keys[0] in ["urls", "vars"]:
            if not isinstance(value, dict):
                if isinstance(value, str) and value.startswith("$"):
                    return self.get(value)
                return value
            env_var_name = value.get("env")
            if env_var_name is not None:
                env_var_value = os.getenv(env_var_name)
                if env_var_value is not None:
                    return env_var_value
            default_value = value.get("default")
            if default_value is not None:
                return default_value

        return None

    def __str__(self):
        return "ConfigData: " + str(self.file)

    def __repr__(self):
        return str(self)


class ConfigFile(YamlFile):

    def get_data(self) -> ConfigData:
        return ConfigData(self.data, self.file)
