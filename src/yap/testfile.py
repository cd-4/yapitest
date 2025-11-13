from pathlib import Path
from test import ApiTest
from typing import List
from utils.yaml import load_yaml


class TestFile:

    def __init__(self, path: Path):
        self.path = path
        self.config = None

    def set_config(self, config) -> None:
        self.config = config

    def find_tests(self) -> List[ApiTest]:
        tests = []
        data = load_yaml(self.path)
        return tests

    def __repr__(self):
        return f"TestFile: {self.path}"
