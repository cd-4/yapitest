import os
from pathlib import Path
from test import ApiTest
from typing import List
from utils.yaml import load_yaml, is_test


class TestFile:

    def __init__(self, path: Path, parent_dirs=None):
        if parent_dirs == None:
            parent_dirs = []
        self.parent_dirs = parent_dirs
        self.path = path
        self.config = None

    def set_config(self, config) -> None:
        self.config = config

    def find_tests(self) -> List[ApiTest]:
        tests = []
        data = load_yaml(self.path)
        for key, test_data in data.items():
            if is_test(key):
                tests.append(ApiTest(self, key, test_data))
        return tests

    def _get_readable_path(self):
        pieces = self.parent_dirs + [self.path.name]
        return os.path.join(*pieces)

    def __repr__(self):
        return f"TestFile: {self._get_readable_path()}"

    def __str__(self):
        return self.__repr__(self)
