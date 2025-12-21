from typing import Any
from pathlib import Path
from ruamel.yaml import YAML

yaml = YAML(typ="safe")


def load_yaml(path: Path):
    return yaml.load(path)


class YamlFile:

    def __init__(self, file: Path):
        self.file = file
        self.data = load_yaml(self.file)

    def get(self, key, default=None) -> Any:
        return self.data.get(key, default)
