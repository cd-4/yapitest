import os
from typing import Dict, Any, List, Optional
from pathlib import Path
from utils.yaml import YamlFile
from test.step import TestStep, ReusableStepGroup


class ConfigData:

    def __init__(self, path: Path, data: Dict[str, Any]):
        self.path = path
        self.raw_data = data
        self.variable_data = self.raw_data.get("variables")
        self.url_data = self.raw_data.get("urls")
        step_groups = {}

        for step_group_key, step_data in self.raw_data.get("step-sets", {}).items():
            run_once = step_data.get("once", False)
            steps = step_data.get("steps", [])

            step_data = [TestStep(-1, sd, None) for sd in steps]

            new_group = ReusableStepGroup(steps, once=run_once)
            step_groups[step_group_key] = new_group
        self.step_groups = step_groups

        self.var_cache = {}
        self.url_cache = {}

    def get_variable(self, var_name: str) -> Optional[Any]:
        if var_name not in self.var_cache:
            var_value = self._get_variable_inner(var_name)
            self.var_cache[var_name] = var_value
        return self.var_cache[var_name]

    def get_url(self, url_name: str) -> Optional[Any]:
        if url_name not in self.url_cache:
            url_value = self._get_url_inner(url_name)
            self.var_cache[url_name] = url_value
        return self.var_cache[url_name]

    def _get_url_inner(self, url_name: str) -> Optional[str]:
        if url_name not in self.url_data:
            return None
        url_value = self.url_data[url_name]
        return self._get_value(url_value)

    def _get_variable_inner(self, var_name: str) -> Optional[Any]:
        if var_name not in self.var_data:
            return None
        var_value = self.var_data[var_name]
        return self._get_value(var_value)

    def _get_value(self, data: Any) -> Any:
        # If the value is a dict, try to get env var, then use default
        if isinstance(data, dict):
            env_var_name = data.get("env")
            if env_var_name is not None:
                env_var_value = os.getenv(env_var_name)
                if env_var_value is not None:
                    return env_var_value
            default_val = data.get("default")
            return default_val

        # Otherwise, return whatever the value is
        return data


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


class ConfigSet(ConfigData):

    VARS_PREFIX = "$vars."

    def __init__(self, configs: List[ConfigData]):
        self.configs = configs
        self.var_cache = {}
        self.url_cache = {}
        self.steps_cache = {}

    def _get_variable_inner(self, var_name: str) -> Optional[Any]:
        for config in reversed(self.configs):
            config_value = config.get_variable(var_name)
            if config_value is not None:
                return config_value
        raise VariableNotFoundException(var_name, self.configs)

    def _get_url_inner(self, url_name: str) -> Optional[str]:
        for config in reversed(self.configs):
            url_value = config.get_url(url_name)
            if url_value is not None:
                return url_value

        raise VariableNotFoundException("urls." + url_name, self.configs)

    def get_step_set(self, step_set_name: str) -> ReusableStepGroup:
        if step_set_name not in self.steps_cache:
            step_set = self._get_step_set_inner(step_set_name)
            if step_set is not None:
                self.steps_cache[step_set_name] = step_set
                return self.steps_cache[step_set_name]
        raise VariableNotFoundException("step-sets." + step_set_name, self.configs)

    def _get_step_set_inner(self, step_set_name: str) -> Optional[ReusableStepGroup]:
        for config in reversed(self.configs):
            step_set_value = config.step_groups.get(step_set_name)
            if step_set_value is not None:
                return step_set_value
        return None
