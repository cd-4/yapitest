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
        reg_json["score"] = 4.2
        reg_json["posts"] = [post.to_json() for post in self.posts]
        return reg_json


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
        return {
            "id": self.id,
            "title": self.title,
            "body": self.content,
            "user_id": self.user_id,
            "meta": {"pinned": False},
        }


@app.route("/", methods=["GET"])
def hello_world():
    return "<p>Hello, World!</p>"


@app.route("/api/healthz", methods=["GET"])
def healthcheck():
    return {"healthy": True}


@app.route("/api/user/create", methods=["POST"])
def create_user():
    if not request.is_json:
        return jsonify({"error": "Request must be JSON"}), 400
    data = request.get_json()
    username = data["username"]
    password = data["password"]
    if username in USERS_BY_USERNAME:
        existing = USERS_BY_USERNAME[username]
        if existing.password == password:
            return {"token": existing.token}
        return jsonify({"error": "Username already taken"}), 409
    token = SampleUser(username, password).token
    return {"token": token}


@app.route("/api/user/login", methods=["POST"])
def login_user():
    if not request.is_json:
        return jsonify({"error": "Request must be JSON"}), 400
    data = request.get_json()
    username = data["username"]
    password = data["password"]
    if username not in USERS_BY_USERNAME:
        return jsonify({"error": "User not found"}), 404
    user = USERS_BY_USERNAME[username]
    if user.password != password:
        return jsonify({"error": "Invalid password"}), 401
    return {"token": user.token}


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


@app.route("/api/user", methods=["GET", "PUT", "DELETE"])
def user():
    token = check_token()
    user = USERS_BY_TOKEN[token]
    if request.method == "GET":
        return user.full_json()
    if request.method == "PUT":
        data = request.get_json()
        old_name = user.name
        new_name = data["username"]
        del USERS_BY_USERNAME[old_name]
        user.name = new_name
        USERS_BY_USERNAME[new_name] = user
        return {"body": "Username updated successfully"}
    if request.method == "DELETE":
        del USERS_BY_ID[user.id]
        del USERS_BY_TOKEN[user.token]
        del USERS_BY_USERNAME[user.name]
        return {"success": True}


@app.route("/api/user/<username>", methods=["GET"])
def get_user(username):
    if username not in USERS_BY_USERNAME:
        return jsonify({"error": "User not found"}), 404
    user = USERS_BY_USERNAME[username]
    return user.full_json()


@app.route("/api/post/list", methods=["GET"])
def list_posts():
    posts = list(POSTS_BY_ID.values())[-20:]
    return {"posts": [p.to_json() for p in posts]}


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


@app.route("/api/post/<int:post_id>", methods=["GET", "DELETE"])
def post(post_id):
    if post_id not in POSTS_BY_ID:
        return jsonify({"error": "Post not found"}), 404
    p = POSTS_BY_ID[post_id]
    if request.method == "GET":
        return p.to_json()
    if request.method == "DELETE":
        token = check_token()
        requesting_user = USERS_BY_TOKEN[token]
        if requesting_user.id != p.user_id:
            return jsonify({"error": "You do not own this post"}), 403
        p.user.posts.remove(p)
        del POSTS_BY_ID[post_id]
        return {"success": True}


if __name__ == "__main__":
    app.run(debug=True, port=8181)
