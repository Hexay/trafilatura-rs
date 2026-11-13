"""Date-like strings for the native dateparser diff (diff_dates.py dateparser): mutations of collected strings and
templates over the month and weekday names of every dateparser locale."""
import random
import re

TIMES = ["10:30", "10.30", "1:05 pm", "3 PM", "23:59:59", "00:00", "12:00 am", "10:30:45,123", "10:30:45.123456",
         "10:30:45,2021", "18h", "24:00", "7:61", "10:30:61", "8 in the evening", "in the morning", "noon"]
ZONES = ["UTC", "GMT", "GMT+2", "UTC-05:00", "+0200", "-03:30", "CET", "CEST", "EST", "PST", "MSK", "IST", "JST",
         "Z", "BST", "AEDT", "(UTC)", "[CET]", "EDT EDT", "GMT+0100 (CET)"]
JOINERS = [" ", ", ", " - ", "-", "/", ".", ". ", " de ", " of ", " le ", " am ", " в ", "年", "月", "日 ", "–"]
WORDS = ["Posted on: ", "Published ", "on ", "le ", "am ", "Updated ", "©", "Copyright ", "Date: ", "Added ",
         "since ", "the ", "this ", "next ", "last ", "ago ", "in ", "today ", "yesterday ", "MMXXI ", "Q3 ",
         "2021 Q1", "г.", "»", "\xa0", "’", "–", "(", ")", "[", "]", "st", "nd", "th", "1st ", "22nd "]


def _names():
    from dateparser.languages.loader import default_loader

    keys = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october",
            "november", "december", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
            "am", "pm", "year", "month", "day", "hour", "minute"]
    names = {}
    for loc in default_loader.get_locales():
        names[loc.shortname] = [w for k in keys for w in loc.info.get(k, []) if isinstance(w, str)]
    return names


def _number(rng):
    return rng.choice([str(rng.randint(1, 31)), f"{rng.randint(1, 12):02}", str(rng.randint(1900, 2030)),
                       f"{rng.randint(0, 99):02}", str(rng.randint(0, 9999)), str(rng.randint(32, 99)),
                       "0", "00", "2026", "26", "30", "29", "31", "13", "12345"])


def _template(rng, names):
    words = names[rng.choice(sorted(names))] or ["may"]
    parts = []
    for _ in range(rng.randint(1, 5)):
        r = rng.random()
        parts.append(rng.choice(words) if r < 0.4 else _number(rng) if r < 0.8 else rng.choice(WORDS).strip())
    s = parts[0]
    for p in parts[1:]:
        s += rng.choice(JOINERS) + p
    if rng.random() < 0.3:
        s += " " + rng.choice(TIMES)
    if rng.random() < 0.15:
        s += " " + rng.choice(ZONES)
    if rng.random() < 0.2:
        s = s.upper() if rng.random() < 0.5 else s.title()
    return s


def _mutate(rng, s):
    r = rng.random()
    if r < 0.35:
        return re.sub(r"\d+", lambda m: _number(rng) if rng.random() < 0.5 else m[0], s)
    if r < 0.5:
        return f"{s} {rng.choice(TIMES)}"
    if r < 0.65:
        return f"{s} {rng.choice(ZONES)}"
    if r < 0.8:
        i = rng.randint(0, len(s))
        return s[:i] + rng.choice(WORDS + JOINERS) + s[i:]
    if r < 0.9:
        words = s.split()
        rng.shuffle(words)
        return " ".join(words)
    return s.lower() if rng.random() < 0.5 else s.upper()


def strings(seeds, n, seed=0):
    """`n` fuzz strings, half mutations of `seeds`, half templates."""
    rng = random.Random(seed)
    names = _names()
    seeds = sorted(seeds)
    out = set()
    while len(out) < n:
        out.add(_mutate(rng, rng.choice(seeds)) if seeds and rng.random() < 0.5 else _template(rng, names))
    return sorted(out)
