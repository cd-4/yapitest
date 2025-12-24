from typing import Union
import requests
from ...test.assertions.assertion import Assertion


class StatusCodeAssertion(Assertion):

    def __init__(
        self,
        response: requests.Response,
        desired_code: Union[int, str],
    ):
        super().__init__()
        self.response = response
        self.desired_code = desired_code

    def _perform_check(self):
        if isinstance(self.desired_code, int):
            return self.response.status_code == self.desired_code
        else:
            code = self.desired_code.lower().rstrip("x")
            return str(self.response.status_code).startswith(code)

    def get_message(self):
        eq = "!="
        if self.passed:
            eq = "=="
        return f"Status code {self.response.status_code} {eq} {self.desired_code}"
