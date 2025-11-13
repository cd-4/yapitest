import os
from typing import List
from pathlib import Path
from utils.yaml import is_test_file
from testfile import TestFile
from test import ApiTest


class TestDiscoverer:
    def __init__(
        self,
        search_paths: List[Path],
        groups: List[str],
        exclude_filters: List[str],
        include_filters: List[str],
    ):
        if len(search_paths) == 0:
            search_paths = [Path(os.getcwd())]
        self.search_paths = search_paths
        self.groups = groups
        self.exclude_filters = exclude_filters
        self.include_filters = include_filters

    def find_test_files_in_dir(self, directory: Path):
        test_files = []
        for entry in directory.iterdir():
            if entry.is_dir():
                test_files.extend(self.find_test_files_in_dir(entry))
            else:
                if is_test_file(entry):
                    test_files.append(entry)
        return test_files

    def find_test_files(self):
        test_files = []
        for path in self.search_paths:
            if path.is_dir():
                test_files.extend(self.find_test_files_in_dir(path))
            else:
                test_files.append(path)
        return [TestFile(tf) for tf in test_files]

    def find_tests(self) -> List[ApiTest]:
        test_files = self.find_test_files()
        tests = []
        for tf in test_files:
            tests.extend(tf.find_tests())
        return tests
