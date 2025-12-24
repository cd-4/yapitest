from typing import List, Optional
from pathlib import Path
from ..utils.yaml import YamlFile
from ..test.config import ConfigData
from ..test.test import Test


class TestFile(YamlFile):

    def __init__(self, file_path: Path, configs: List[ConfigData]):
        super().__init__(file_path)
        self.configs = configs
        self.config = self._get_config()

    def _get_parent_config(self) -> Optional[ConfigData]:

        for config in sorted(
            self.configs,
            key=lambda x: len(str(x.file)),
            reverse=True,
        ):
            if self.file.is_relative_to(config.file.parent):
                return config
        return None

    def _get_config(self) -> Optional[ConfigData]:
        parent_config = self._get_parent_config()
        config_data = self.data.get("config")
        if config_data is not None:
            return ConfigData(config_data, self.file, parent_config)
        return parent_config

    def get_tests(self):
        tests = []
        for key, test_data in self.data.items():
            lower_key = key.lower()
            if not (lower_key.startswith("test") or lower_key.endswith("test")):
                continue
            test = Test(key, test_data, self.config)
            tests.append(test)
        return tests
