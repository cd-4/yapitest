class ApiTest:

    def __init__(self, testfile, name, data):
        self.name = name
        self.testfile = testfile

    def __repr__(self):
        file_path = self.testfile._get_readable_path()
        return "ApiTest: " + file_path + ":" + self.name

    def __str__(self):
        return self.__repr__()
