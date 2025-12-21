class RequiredParameterNotDefined(Exception):

    def __init__(self, parameter_name: str):
        self.parameter_name = parameter_name
        msg = f"Missing required parameter `{self.parameter_name}`"
        super().__init__(msg)
