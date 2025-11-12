from typing import List
from pathlib import Path


class TestDiscoverer:
    def __init__(
        self,
        search_paths: List[Path],
        groups: List[str],
        exclude_filters: List[str],
        include_filters: List[str],
    ):
        self.search_paths = search_paths
        self.groups = groups
        self.exclude_filters = exclude_filters
        self.include_filters = include_filters

    def find_tests(self):
        pass
