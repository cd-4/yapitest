import os
import re
from typing import List, Callable
from pathlib import Path


def find_files(search_paths: List[Path], is_valid: Callable[Path, bool]) -> List[Path]:

    output_files = []
    if len(search_paths) == 0:
        search_paths = [Path(os.getcwd())]

    for search_path in search_paths:
        for dirpath, dirnames, filenames in os.walk(search_path):
            dir_path = Path(dirpath)
            for file in filenames:
                file_path = dir_path / file
                if is_valid(file_path):
                    output_files.append(file_path)

    return output_files


def is_test_file(file: Path):
    valid_yaml_re = r"^test[-_].*\.ya?ml$|^.*test(?:s)?\.ya?ml$"
    return re.match(valid_yaml_re, file.name)


def is_config_file(file: Path):
    valid_config_file_re = r"^(yapitest-)?config.ya?ml$"
    return re.match(valid_config_file_re, file.name)


def find_test_files(search_paths: List[Path]):
    return find_files(search_paths, is_test_file)


def find_config_files(search_paths: List[Path]):
    return find_files(search_paths, is_config_file)
