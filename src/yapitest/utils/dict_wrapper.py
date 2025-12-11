from typing import Dict, Any, List, Optional


class Gettable:

    def __init__(self):
        pass

    def get(self, key_s: Any) -> Optional[Any]:
        class_name = self.__class__.__name__
        raise NotImplementedError(f"`{class_name}.get` (Gettable) not defined")


class DeepDict(Gettable):

    def __init__(self, data: Dict):
        self.data = data

    def set_value(self, key: Any, value: Any):
        self.data[key] = value

    def get(self, key_s: Any) -> Optional[Any]:
        if isinstance(key_s, str) and key_s.startswith("$"):
            keys = key_s[1:].split(".")
            return self._get_keys(keys)
        return self._get_keys([key_s])

    def _get_keys(self, keys: List[str]) -> Optional[Any]:
        data = self.data
        for key in keys:
            data = data.get(key)
            if data is None:
                return None
        return data
