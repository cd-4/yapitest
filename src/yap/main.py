from config import YapConfig
from pathlib import Path
from argparse import ArgumentParser
from discover import TestDiscoverer


class YapProject:

    def __init__(self, args):
        self.args = args
        self.config = YapConfig.find_config()
        self.discoverer = TestDiscoverer(
            args.test_paths,
            args.group,
            args.exclude,
            args.include,
        )

    def run(self):
        tests = self.discoverer.find_tests()
        pass


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
