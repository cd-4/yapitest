class InvalidTestException:

    def __init__(self, test, message):
        super.__init__(f"Invalid Test: {test} " + message)


class ApiTestStep:

    def __init__(self, data, test):
        self.raw_data = data
        self.api_test = test
        # Optional
        self.id = data.get("id")
        # Mandatory
        self.path = data.get("path")
        # Can be overridden, default to base URL
        self.url = data.get("url")
        # Default to GET
        self.method = data.get("method", "GET").lower()
        # If None, don't send any data
        self.data = data.get("data", None)
        # If None, don't send any headers
        self.headers = data.get("headers", {})
        # If None, don't check anything
        self.assertions = data.get("assert", None)


class ApiTest:

    def __init__(self, testfile, name, data):
        self.name = name
        self.testfile = testfile
        self.data = data

    def __repr__(self) -> str:
        file_path = self.testfile._get_readable_path()
        return "ApiTest: " + file_path + ":" + self.name

    def __str__(self) -> str:
        return self.__repr__()

    def generate_steps(self) -> None:
        if "steps" not in self.data:
            raise InvalidTestException(test, "`steps` not defined")
        self.test_steps = []
        self.steps_by_id = {}
        for step_data in self.data["steps"]:
            step = ApiTestStep(step_data, self)
            self.steps_by_id[step.id] = step
            self.test_steps.append(step)
            print(step)

    def run(self):
        self.generate_steps()
