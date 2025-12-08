import os
from pathlib import Path


def parent_paths(path: Path):
    segs = str(path).split(os.sep)
    amt = len(segs)
    while amt > 0:
        yield Path(os.sep.join(segs[:amt]) + os.sep)
        amt -= 1
