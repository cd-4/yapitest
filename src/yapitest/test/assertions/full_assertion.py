from typing import Dict
from ...utils.dict_wrapper import flatten_dict
from ...test.assertions.assertion import Assertion


class FullAssertion(Assertion):

    def __init__(self, response_data: Dict, expected_data: Dict):
        self.response_data = response_data
        self.expected_data = expected_data

    def _perform_check(self) -> bool:
        # Nobody in their right mind would put this as a key in JSON
        key_joiner = "YAPITEST_JOIN_KEY"

        flat_response = [
            key_joiner.join(f[0]) for f in flatten_dict(self.response_data)
        ]
        flat_expected = [
            key_joiner.join(f[0]) for f in flatten_dict(self.expected_data)
        ]

        return set(flat_response) == set(flat_expected)
