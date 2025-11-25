from pathlib import Path
import os
from typing import Dict, List


class TestSetup:

    def __init__(self, path: Path, data: Dict):
        self.path = path
        self.data = data


class TestSetupFinder:

    def __init__(self):
        self.setups = {}

    def find_setups(self, dirs: List[Path]):
        if len(dirs) == 0:
            dirs = [Path(os.getcwd())]
        for dir in dirs:
            self.find_in_dir(dir)
        return self.setups

    def find_in_dir(self, dir: Path, prev_dirs=None):
        if prev_dirs == None:
            prev_dirs = []
        for entry in dir.iterdir():
            if entry.is_dir():
                self.find_in_dir(entry, prev_dirs + [entry])
            else:
                if entry.name in [
                    "yap-setups.yaml",
                    "yap-setups.yml",
                ]:
                    self.load_setups_from_file(entry, prev_dirs)
                    print(prev_dirs, entry.name)

    def load_setups_from_file(self, file: Path, prev_dirs=List[str]):
        raise Exception(file)
