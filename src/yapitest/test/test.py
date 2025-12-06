from pathlib import Path
from typing import Dict, List
from test.config import ConfigData, ConfigSet


class Test:

    def __init__(self, name: str, data: Dict, file: Path, configs: List[ConfigData]):
        self.name = name
        self.raw_data = data
        self.groups = self.raw_data.get("groups", [])

        test_config = self.raw_data.get("config", None)
        if test_config is not None:
            configs.append(ConfigData(self.file, test_config))
        self.config_set = ConfigSet(configs)

    def run(self):
        pass
