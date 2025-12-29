class Assertion:

    def __init__(self):
        self.checked = False
        self.passed = False

    def get_message(self, verbose=False) -> str:
        return ""

    def _perform_check(self) -> bool:
        return False

    def check(self) -> bool:
        passes = self._perform_check()
        if passes:
            self.passed = True
        else:
            self.passed = False
        self.checked = True
        return passes

    def get_json(self):
        return {
            "passed": self.passed,
            "message": self.get_message(),
        }
