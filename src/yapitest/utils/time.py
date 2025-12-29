import time


def get_time_ms():
    time_ns = time.time_ns()
    time_ms = time_ns // 1_000_000
    return time_ms
