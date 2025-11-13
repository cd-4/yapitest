import os
from pathlib import Path
from typing import Optional
from utils.yaml import load_yaml
from utils.dict_wrapper import DictWrapper


class VariableNotDefinedException(Exception):
    def __init__(self, var_name):
        self.message = f"Variable not defined: {var_name}"
        super().__init__(self.message)


class YapConfig:

    ROOT_MARKERS = [".git"]
    CONFIG_NAME = "yap-config"

    @staticmethod
    def find_config_in_dir(directory) -> Path:
        configs = [YapConfig.CONFIG_NAME + ".yaml", YapConfig.CONFIG_NAME + ".yml"]
        for config in configs:
            config_path = directory / config
            if config_path.exists():
                return directory / config
        return None

    @staticmethod
    def find_config():
        cwd = Path(os.getcwd())
        found_config = YapConfig.find_config_in_dir(cwd)
        if found_config:
            return YapConfig(found_config)
        while found_config is None:
            cwd = cwd.parent
            found_config = YapConfig.find_config_in_dir(cwd)
            if found_config:
                return YapConfig(found_config)

            for root_marker in YapConfig.ROOT_MARKERS:
                root_check = cwd / root_marker
                if root_check.exists():
                    return YapConfig()
        return YapConfig()

    def __init__(self, config_file: Optional[Path] = None):
        self.config_data = {}
        if config_file:
            print(f"Using Config: {config_file}")
            self.config_data = load_yaml(config_file)

        self.vars = self.config_data.get("variables")
        self.urls = DictWrapper(self.config_data.get("urls", {}), self)

    def get_variable(self, var_name):
        if var_name not in self.vars:
            raise VariableNotDefinedException(var_name)

        var_data = self.vars[var_name]
        if isinstance(var_data, dict):
            if "env" in var_data:
                var_env = var_data["env"]
                output = os.getenv(var_env, var_data.get("default"))
                if output is None:
                    raise VariableNotDefinedException(var_name)
                return output

        else:
            return str(var_data)
