import os
from pathlib import Path


class YapConfig:

    ROOT_MARKERS = [".git"]
    CONFIG_NAME = "yap-config"

    @staticmethod
    def find_config_in_dir(directory) -> Path:
        configs = [YapConfig.CONFIG_NAME + ".yaml", YapConfig.CONFIG_NAME + ".yml"]
        for config in configs:
            config_path = directory / config
            if config_path.exists():
                return directory / config
        return None

    @staticmethod
    def find_config():
        cwd = Path(os.getcwd())
        found_config = YapConfig.find_config_in_dir(cwd)
        while found_config is None:
            cwd = cwd.parent
            found_config = YapConfig.find_config_in_dir(cwd)
            if found_config:
                return found_config

            for root_marker in YapConfig.ROOT_MARKERS:
                root_check = cwd / root_marker
                if root_check.exists():
                    return None
        return None

    def __init__(self, config_file: Path):
        pass


if __name__ == "__main__":
    YapConfig.find_config()
