from typing import List
from pathlib import Path
from utils.yaml import YamlFile
from test.test import Test
from test.config import ConfigData


class TestFile(YamlFile):

    def __init__(self, file: Path, configs: List[ConfigData]):
        super().__init__(file)
        self.configs = configs
        file_config = self.get("config", None)
        if file_config is not None:
            self.configs.append(ConfigData(self.file, file_config))

    def _is_key_test(self, key: str):
        return key.startswith("test") or key.endswith("test")

    def get_tests(self) -> List[Test]:
        tests = []

        for test_name, test_data in self.data.items():
            if not self._is_key_test(test_name.lower()):
                continue
            new_test = Test(test_name, test_data, self.file, self.configs)
            tests.append(new_test)

        return tests
