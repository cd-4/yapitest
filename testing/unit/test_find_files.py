import os
from pathlib import Path
from yapitest.find import finder
from utils.test_dir import get_test_dir
from typing import Dict
from contextlib import contextmanager


def prepare_test_dir_rec(cur_path: Path, dir_data: Dict):
    dirs = dir_data.get("dirs", {})
    files = dir_data.get("files", [])

    for dir, data in dirs.items():
        dir_path = cur_path / dir
        os.mkdir(dir_path)
        prepare_test_dir_rec(dir_path, data)

    for file in files:
        file_path = cur_path / file
        with open(file_path, "w+") as f:
            f.write("test: 1")


@contextmanager
def prepare_test_dir():
    dirs = {
        "dirs": {
            "one": {
                "dirs": {
                    "subdir-one": {
                        "files": [
                            "test_one.yml",
                            "test-one.yml",
                        ]
                    },
                },
                "files": [
                    "test_one.yaml",
                    "test-one.yaml",
                ],
            },
            "two": {
                "files": [
                    "two-test.yaml",
                    "two_test.yaml",
                ],
                "dirs": {
                    "sub": {
                        "files": [
                            "three_test.yml",
                            "three-test.yml",
                        ],
                    },
                    "sub2": {
                        "files": [
                            "four_tests.yml",
                            "four_tests.yml",
                            "five-tests.yaml",
                            "five-tests.yaml",
                        ],
                    },
                },
            },
        },
        "files": [
            "yap-config.yaml",
            "yap-config.yml",
        ],
    }

    with get_test_dir() as test_dir:
        prepare_test_dir_rec(test_dir, dirs)
        yield test_dir


def test_find_files():

    with prepare_test_dir() as test_dir:
        found_files = finder.find_test_files([test_dir])
        assert found_files == []
