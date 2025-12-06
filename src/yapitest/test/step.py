from typing import Dict, List


class TestStep:

    def __init__(self, data: Dict):
        self.raw_data = data

    def pre_run(self):
        pass

    def post_run(self):
        pass

    def run(self):
        self.pre_run()
        self.run_internal()
        self.post_run()

    def run_internal(self):
        pass


class ReusableStepGroup:

    def __init__(self, steps: List[TestStep], once=False):
        self.run_once = once
        self.steps = steps
        self.has_run = False

    def run(self):

        if self.run_once and self.has_run:
            return

        for step in self.steps:
            step.run()
