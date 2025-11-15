class VariableDictWrapper:

    VAR_PREFIX = "$vars."

    def __init__(self, data, config):
        self.data = data
        self.config = config

    def get(self, key, default=None):
        if key in self.data:
            return self[key]
        return default

    def __getitem__(self, key):
        value = self.data.__getitem__(key)
        if isinstance(value, str):
            if value.startswith(VariableDictWrapper.VAR_PREFIX):
                var_name = value[len(VariableDictWrapper.VAR_PREFIX) :]
                return self.config.get_variable(var_name)
        return output


def flatten_dict(d, parent_key="", sep="."):
    items = []
    for k, v in d.items():
        new_key = parent_key + sep + k if parent_key else k
        if isinstance(v, dict):
            items.extend(flatten_dict(v, new_key, sep=sep).items())
        else:
            items.append((new_key, v))
    return dict(items)
