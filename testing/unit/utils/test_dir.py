import os
import shutil
from pathlib import Path
from contextlib import contextmanager


@contextmanager
def get_test_dir():
    test_dir_path = Path(__file__).parent / "_TEMP_"
    # Code to acquire resource, e.g.:
    if os.path.exists(test_dir_path):
        shutil.rmtree(test_dir_path)

    os.makedirs(test_dir_path, exist_ok=True)

    yield test_dir_path
    shutil.rmtree(test_dir_path)
