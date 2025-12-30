import sys
from pathlib import Path
from argparse import ArgumentParser
from typing import List
import json
from .find.finder import find_test_files, find_config_files
from .test.config import ConfigFile
from .test.file import TestFile
from .utils.paths import parent_paths
from .utils.time import get_time_ms
from .utils import console


class YapProject:

    def __init__(self, args):
        self.args = args
        # self.config = YapConfig.find_config()
        self.configs = self.find_configs()
        self.tests = self.find_tests()

    def print_summary(self, summary):
        print(f"\n{console.BOLD_MAGENTA}Summary{console.RESET}")

        print(f"  {summary["passed"]} Tests Passed{console.RESET}")
        print(f"  {summary["failed"]} Tests Failed{console.RESET}")

        print(f"\n{console.BOLD_MAGENTA}Failure Summary:{console.RESET}")
        for test in self.tests:
            if test.status == "failed":
                test.print_fail_summary()

    def run(self):
        print(f"{console.BOLD_MAGENTA}Running Tests{console.RESET}")
        start_time = get_time_ms()

        test_results = []
        for test in self.tests:
            result = test.run()
            test_results.append(result)

        end_time = get_time_ms()

        summary = {
            "start": start_time,
            "stop": end_time,
            "tests": len(self.tests),
            "passed": 0,
            "failed": 0,
            "pending": 0,
            "skipped": 0,
            "other": 0,
        }
        for test in self.tests:
            summary[test.status] += 1

        self.print_summary(summary)

        return {
            "tool": "yapitest",
            "tests": test_results,
            "summary": summary,
        }

    def contains_texts(self, name: str, texts: List[str]) -> bool:
        for text in texts:
            if text in name:
                return True
        return False

    def find_tests(self):
        tests = []
        # TODO: Filter Test Files
        # TODO: Filter Tests
        for file in find_test_files(self.args.test_paths):
            test_file = TestFile(file, self.configs)
            tests.extend(test_file.get_tests())

        if self.args.group:
            tests = [t for t in tests if t.in_groups(self.args.group)]

        if self.args.include:
            tests = [t for t in tests if self.contains_texts(t.name, self.args.include)]
        if self.args.exclude:
            tests = [
                t for t in tests if not self.contains_texts(t.name, self.args.exclude)
            ]

        return tests

    def find_configs(self):
        all_configs = find_config_files(self.args.test_paths)
        sorted_configs = sorted(all_configs, key=lambda x: len(str(x)))
        config_objs = [ConfigFile(cf).get_data() for cf in sorted_configs]

        configs_by_dir = {}
        for config in config_objs:
            configs_by_dir[config.file.parent] = config

        for config in config_objs:
            found = False
            for ppath in parent_paths(config.file.parent.parent):
                parent_config = configs_by_dir.get(ppath)
                if parent_config is not None:
                    config.set_parent(parent_config)
                    break

        return config_objs


def get_parser():
    parser = ArgumentParser(
        prog="yapitest",
        description="Yaml-based API testing framework",
        epilog="Text at the bottom of help",
    )

    parser.add_argument(
        "test_paths", help="Files/Directories that contains tests", type=Path, nargs="*"
    )
    parser.add_argument(
        "-g",
        "--group",
        action="append",
        required=False,
        help="Specify groups of tests. (Can be used multiple times)",
    )
    parser.add_argument(
        "-x",
        "--exclude",
        action="append",
        required=False,
        help="Test names with matching subsstrings will not be run. (Can be used multiple times)",
    )
    parser.add_argument(
        "-i",
        "--include",
        action="append",
        required=False,
        help="Test names with matching subsstrings will be run. (Can be used multiple times)",
    )
    return parser


def main():
    args = get_parser().parse_args()
    project = YapProject(args)
    results = project.run()

    has_failures = results["summary"]["failed"] > 0

    with open("yapitest-results.json", "w+") as f:
        json.dump(results, f)

    if has_failures:
        sys.exit(1)
    else:
        sys.exit(0)


if __name__ == "__main__":
    main()
