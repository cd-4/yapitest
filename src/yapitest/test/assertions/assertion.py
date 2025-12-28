class Assertion:

    def __init__(self):
        self.checked = False
        self.passed = False

    def get_message(self, verbose=False) -> str:
        return ""

    def _pass(self) -> None:
        self.checked = True
        self.passed = True

    def _fail(self) -> None:
        self.checked = True
        self.passed = False

    def _perform_check(self) -> bool:
        return False

    def check(self) -> bool:
        passes = self._perform_check()
        if passes:
            self._pass()
        else:
            self._fail()
        return passes

    def get_json(self):
        return {
            "passed": self.passed,
            "message": self.get_message(),
        }
