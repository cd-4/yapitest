from pathlib import Path
from argparse import ArgumentParser
from find.finder import find_test_files, find_config_files
from test.config import ConfigFile
from utils.paths import parent_paths


class YapProject:

    def __init__(self, args):
        self.args = args
        # self.config = YapConfig.find_config()
        self.configs = self.find_configs()
        # self.tests = self.find_tests()

    def run(self):
        return
        for test in self.tests:
            print("Running Test: " + test.name)

            test.run()
            break

    def find_tests(self):
        test_files = []

        for file in find_test_files(self.args.test_paths):
            configs = []
            for config in self.configs:
                config_dir = config.path.parent
                if file.is_relative_to(config_dir):
                    configs.append(config)

            configs.sort(key=lambda x: len(str(x.path)))

        # TODO: Filter Test Files

        all_tests = []
        for tf in test_files:
            all_tests.extend(tf.get_tests())

        # TODO: Filter Tests

        return all_tests

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
        prog="Yap Test",
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


if __name__ == "__main__":
    args = get_parser().parse_args()
    project = YapProject(args)
    project.run()
