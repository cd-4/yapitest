from flask import Flask, request, jsonify, abort
import string
import random

app = Flask(__name__)


USERS_BY_ID = {}
USERS_BY_TOKEN = {}
USERS_BY_USERNAME = {}

POSTS_BY_ID = {}


def generate_token():
    characters = string.ascii_letters + string.digits
    random_string = "".join(random.choices(characters, k=20))
    return random_string


def random_username() -> str:
    nouns = [
        "man",
        "woman",
        "dog",
        "cat",
        "house",
        "tiger",
        "lion",
        "bear",
        "tree",
        "plant",
        "flower",
        "bison",
        "cow",
    ]
    return random.choice(nouns) + "-" + random.randing(1, 99)


def check_token() -> None:
    token = request.headers.get("API-Token")
    if token not in USERS_BY_TOKEN:
        abort(403, description="Invalid or unspecified token")
    return token


class SampleUser:

    def __init__(self, name: str, password: str):
        self.name = name
        self.id = len(USERS_BY_ID) + 1
        self.token = generate_token()
        self.password = password
        self.posts = []
        USERS_BY_ID[self.id] = self
        USERS_BY_TOKEN[self.token] = self
        USERS_BY_USERNAME[self.name] = self

    def to_json(self):
        return {
            "name": self.name,
            "id": self.id,
        }

    def full_json(self):
        reg_json = self.to_json()
        reg_json["posts"] = [post.to_json() for post in self.posts]
        return reg_json

    @staticmethod
    def random():
        return SampleUser(random_username(), random_string())


class SamplePost:

    def __init__(self, title: str, content: str, user_id: int):
        self.id = len(POSTS_BY_ID) + 1
        self.title = title
        self.content = content
        self.user_id = user_id
        self.user.posts.append(self)
        POSTS_BY_ID[self.id] = self

    @property
    def user(self):
        return USERS_BY_ID[self.user_id]

    def to_json(self):
        return {"title": self.title, "body": self.content}


@app.route("/", methods=["GET"])
def hello_world():
    return "<p>Hello, World!</p>"


@app.route("/api/healthz", methods=["GET"])
def healthcheck():
    return {"healthy": True}


# TESTED
@app.route("/api/user/create", methods=["POST"])
def create_user():
    if not request.is_json:
        return jsonify({"error": "Request must be JSON"})
    data = request.get_json()
    # time.sleep(1)
    username = data["username"]
    password = data["password"]
    token = SampleUser(username, password).token
    return {"token": token}


# TESTED
@app.route("/api/token/check", methods=["POST"])
def check_token_endpoint():
    if not request.is_json:
        return jsonify({"error": "Request must be JSON"})
    data = request.get_json()
    return {"success": data["token"] in USERS_BY_TOKEN}


@app.route("/api/user/list", methods=["GET"])
def get_users():
    users = list(USERS_BY_ID.values())[:10]
    users_json = [u.to_json() for u in users]
    return {"users": users_json}


@app.route("/api/user", methods=["PUT", "GET"])
def user():
    token = check_token()
    user = USERS_BY_TOKEN[token]
    if request.method == "GET":
        return user.full_json()
    if request.method == "PUT":
        data = request.get_json()
        user.name = data["username"]
        return {"body": "Username updated successfully"}
    # This won't be hit


@app.route("/api/user/<username>", methods=["GET"])
def get_user(username):
    user = USERS_BY_USERNAME[username]
    output = user.full_json()
    return output


@app.route("/api/post/create", methods=["POST"])
def create_post():
    if not request.is_json:
        return jsonify({"error": "Request must be JSON"})
    data = request.get_json()
    title = data["title"]
    body = data["body"]
    token = check_token()
    user = USERS_BY_TOKEN[token]
    user_id = user.id
    new_post = SamplePost(title, body, user_id)
    return {"post_id": new_post.id}, 201


@app.route("/api/post/<int:post_id>", methods=["GET"])
def get_post(post_id):
    post = POSTS_BY_ID[post_id]
    return post.to_json()


if __name__ == "__main__":
    app.run(debug=True, port=8181)
