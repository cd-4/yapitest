import os
from typing import Dict, Any, List
from pathlib import Path
from utils.yaml import YamlFile
from test.step import TestStep


class ConfigData:

    def __init__(self, path: Path, data: Dict[str, Any]):
        self.path = path
        self.raw_data = data
        self.variable_data = self.raw_data.get("variables")
        self.url_data = self.raw_data.get("urls")
        step_groups = {}

        for step_group_key, step_data in self.raw_data.get("steps", {}).items():
            run_once = step_data.get("once", False)
            steps = step_data.get("steps", [])

            step_data = [TestStep(sd) for sd in steps]

            new_group = ReusableStepGroup(steps, once=run_once)
            step_groups[step_group_key] = new_group
        self.step_groups = step_groups


class VariableNotFoundException(Exception):
    def __init__(self, variable_name: str, configs: List[ConfigData]):
        config_files = [" - " + str(cd.path) for cd in reversed(configs)]
        self.message = f"Variable ({variable_name}) Not Found in:\n" + "\n".join(
            config_files
        )
        super().__init__(self.message)


class ConfigFile(YamlFile):

    def __init__(self, file: Path):
        super().__init__(file)

    def get_data(self) -> ConfigData:
        return ConfigData(self.file, self.data)


class ConfigSet:

    def __init__(self, configs: List[ConfigData]):
        self.configs = configs
        self.get_variable("asdf")

    def get_variable(self, var_name: str):
        for config in reversed(self.configs):
            if var_name in config.variable_data:
                var_data = config.variable_data[var_name]
                if isinstance(var_data, dict):
                    env_var_name = var_data.get("env")
                    if env_var_name is not None:
                        env_var_value = os.getenv(env_var_name)
                        if env_var_value is not None:
                            return env_var_value
                    default = var_data.get("default")
                    if default is not None:
                        return default

        raise VariableNotFoundException(var_name, self.configs)
