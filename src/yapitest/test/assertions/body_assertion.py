from typing import Dict, Any, List, Tuple, Optional, Union
import re
from ...utils.dict_wrapper import DeepDict, flatten_dict
from ...test.assertions.assertion import Assertion


class BodyAssertion(Assertion):

    def __init__(
        self,
        keys: List[Any],
        desired_value: Any,
        response_data: Dict,
        parent: "TestStep",
        prior_steps: Dict[str, "TestStep"],
    ):
        super().__init__()
        self.keys = keys
        self.desired_value = desired_value
        self.response_data = response_data
        self.parent = parent
        self.prior_steps = prior_steps

    def _get_desired_type(self, type_str: str):
        type_str = type_str[1:].lower()  # Remove + in front
        if type_str in ["str", "string"]:
            return str
        elif type_str in ["bool", "boolean"]:
            return bool
        elif type_str in ["int", "integer"]:
            return int
        elif type_str in ["float", "flt"]:
            return float
        elif type_str in ["array", "arr", "list"]:
            return list
        elif type_str in ["dict", "dic", "dictionary", "map"]:
            return dict
        raise Exception(f"Desired type {type_str} invalid")

    def _check_type(self, value: Any, desired_value: str):
        desired_type = self._get_desired_type(desired_value)
        return isinstance(value, desired_type)

    def _check_single_value(self, value: Any, desired_value: Any) -> bool:
        if isinstance(desired_value, str) and desired_value.startswith("+"):
            return self._check_type(value, desired_value)
        else:
            return value == desired_value

    def is_length(self) -> Optional[Tuple[bool, str]]:
        reg = r"len\((.*)\)"
        match = re.match(reg, self.keys[-1])
        if match is None:
            return False, "NO MATCH"
        return True, match.group(1)

    def _check_length(self, value: Any, length: Union[int, str]) -> bool:
        # Check for exact length
        if isinstance(length, int):
            return len(value) == length

        length = length.strip().replace(" ", "")

        prefix = ""
        prefixes = [">=", "<=", ">", "<"]
        for pref in prefixes:
            if length.startswith(pref):
                prefix = pref
                break

        if len(prefix) == 0:
            raise Exception(f"Unable to parse `len` assertion {length}")

        expected_value = int(length[len(prefix) :])

        if prefix == ">=":
            return len(value) >= expected_value
        elif prefix == "<=":
            return len(value) <= expected_value
        elif prefix == ">":
            return len(value) > expected_value
        elif prefix == "<":
            return len(value) < expected_value
        else:
            raise Exception(f"Unable to parse `len` assertion {length}")

    def _sanitize(self, var: Any):
        if not (isinstance(var, str) and var.startswith("$")):
            return var

        return self.parent._get_special_value(var, self.prior_steps)

    def _perform_check(self) -> bool:
        self.bad_assertions = []

        is_length, last_token = self.is_length()
        if is_length:
            keys = self.keys[:-1] + [last_token]
            value = self.response_data._get_keys(keys)
            res = self._check_length(value, self.desired_value)
            return res
        else:
            value = self.response_data._get_keys(self.keys)
            desired_value = self._sanitize(self.desired_value)
            res = self._check_single_value(value, desired_value)
            return res

        return not fails

    def get_message(self):
        return f"{'.'.join(self.keys)} is {self.desired_value}"


def get_body_assertions(
    response_data: Dict,
    desired_data: Dict,
    parent: "TestStep",
    prior_steps: Dict[str, "TestStep"],
) -> List[BodyAssertion]:
    response_data = DeepDict(response_data)
    desired_data = flatten_dict(desired_data)

    assertions = []
    for desired_keys, desired_value in desired_data:
        new_assertion = BodyAssertion(
            desired_keys,
            desired_value,
            response_data,
            parent,
            prior_steps,
        )
        assertions.append(new_assertion)
    return assertions
