from typing import Dict, Optional, List, Any
import os
from pathlib import Path
from utils.dict_wrapper import DeepDict
from utils.yaml import YamlFile


class ConfigData(DeepDict):

    def __init__(self, data: Dict, file: Path, parent: Optional["ConfigData"] = None):
        super().__init__(data)
        self.file = file
        self.parent = parent

    def set_parent(self, parent: "ConfigData") -> None:
        self.parent = parent

    def _get_keys(self, keys: List[str]) -> Optional[Any]:
        value = super()._get_keys(keys)
        if value is None and self.parent is not None:
            return self.parent._get_keys(keys)

        if keys[0] in ["urls", "vars"]:
            if not isinstance(value, dict):
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
