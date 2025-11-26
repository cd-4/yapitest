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
                    "config.yaml",
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
                            "config.yml",
                        ],
                    },
                    "sub2": {
                        "files": [
                            "four_tests.yml",
                            "four-tests.yml",
                            "five-tests.yaml",
                            "five_tests.yaml",
                        ],
                    },
                },
            },
        },
        "files": [
            "yapitest-config.yaml",
            "yapitest-config.yml",
        ],
    }

    with get_test_dir() as test_dir:
        prepare_test_dir_rec(test_dir, dirs)
        yield test_dir


def test_find_files():

    with prepare_test_dir() as test_dir:
        root_dir = str(test_dir)
        found_files = finder.find_test_files([test_dir])
        strpaths = [str(x)[len(root_dir) :] for x in found_files]

        expected_paths = [
            "/one/subdir-one/test_one.yml",
            "/one/subdir-one/test-one.yml",
            "/one/test-one.yaml",
            "/one/test_one.yaml",
            "/two/sub/three_test.yml",
            "/two/sub/three-test.yml",
            "/two/sub2/four-tests.yml",
            "/two/sub2/four_tests.yml",
            "/two/sub2/five_tests.yaml",
            "/two/sub2/five-tests.yaml",
            "/two/two-test.yaml",
            "/two/two_test.yaml",
        ]
        assert len(strpaths) == len(expected_paths)
        for ex in expected_paths:
            assert ex in strpaths


def test_find_configs():
    with prepare_test_dir() as test_dir:
        root_dir = str(test_dir)
        found_files = finder.find_config_files([test_dir])
        strpaths = [str(x)[len(root_dir) :] for x in found_files]

        expected_paths = [
            "/yapitest-config.yml",
            "/yapitest-config.yaml",
            "/one/config.yaml",
            "/two/sub/config.yml",
        ]
        assert len(strpaths) == len(expected_paths)
        for ex in expected_paths:
            assert ex in strpaths
