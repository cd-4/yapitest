import random

NOUNS = [
    "dog",
    "cat",
    "human",
    "monster",
    "lion",
    "tiger",
    "bear",
    "house",
    "car",
    "cart",
    "horse",
    "penguin",
    "television",
    "potato",
    "radish",
    "lemon",
    "lime",
    "orange",
    "apple",
    "banana",
    "pumpkin",
    "field",
    "grass",
    "gasoline",
    "tree",
    "maple",
    "pine",
    "computer",
    "calculator",
    "racecar",
    "rocket",
    "boat",
    "ship",
    "cruiser",
    "bruiser",
    "runner",
    "walker",
]

ADJECTIVES = [
    "big",
    "small",
    "smelly",
    "fragrant",
    "tasty",
    "wide",
    "short",
    "thin",
    "fat",
    "wonderful",
    "terrible",
    "magnificent",
    "magnanimous",
    "skinny",
    "medium",
    "awful",
    "bad",
    "good",
    "fantastic",
    "mean",
    "nice",
    "intentional",
    "accidental",
]


def random_noun():
    return random.choice(NOUNS)


def random_adjective():
    return random.choice(ADJECTIVES)


def rand_bool():
    random_bit = random.getrandbits(1)
    return bool(random_bit)


def random_username(length=-1, split="-_"):
    has_splitter = rand_bool()
    noun = random_noun()
    adjective = random_adjective()
    number = random.randint(0, 99)

    if split and has_splitter():
        splitter = random.choice(split)
    else:
        splitter = ""

    output = adjective + splitter + noun + str(number)
    if len(output) > length and length != -1:
        output = output[:length]
    return output


def random_password(length=-1):
    return random_username(length=length, split="")
