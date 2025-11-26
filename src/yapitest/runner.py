frmo test import ApiTest

class TestRunner:

    def __init__(self, tests:ApiTest):
        self.tests = tests

    def run_in_sequence(self):
        for test in self.tests:
            test.run()

    def run_in_parallel(self):
        pass
