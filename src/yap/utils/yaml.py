from pathlib import Path
from ruamel.yaml import YAML

yaml = YAML(typ="safe")


def is_yaml(path: Path):
    n = path.name
    return n.endswith(".yaml") or n.endswith(".yml")


def is_test_file(path: Path):
    if is_yaml(path):
        stem = path.stem
        prefixes = ["test-", "test_"]
        suffixes = ["-test", "_test", "-tests", "_tests"]
        for pre in prefixes:
            if stem.startswith(pre):
                return True
        for suf in suffixes:
            if stem.endswith(suf):
                return True
    return False


def load_yaml(path: Path):
    return yaml.load(path)
