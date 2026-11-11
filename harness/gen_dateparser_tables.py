"""Tables for the native dateparser shortcut (see crates/trafilatura-rs/src/dates/dateparser/mod.rs).
usage: gen_dateparser_tables.py > crates/trafilatura-rs/src/dates/dateparser/tables.txt

Per locale, the ASCII words that may survive in a date string the locale accepts: maximal runs of ASCII letters of
its dictionary keys and of every string its relative-time and simplification regexes can match (the language is
over-approximated: lookarounds are dropped, wide classes become wildcards). Lines:
  L <locale> words <word>...           a surviving word must be one of these
  L <locale> pieces <word>...          no_word_spacing locales: a surviving word must be a concatenation of these
  L <locale> any                       nothing can be excluded (a wildcard reached the word list)
  T <word>...                          timezone names (pop_tz_offset_from_string may remove them)
  M <word>...                          htmldate's month_words() (has_date_cue)"""
import string
import sys
from functools import cache
from importlib.metadata import version
from re import _constants as C
from re import _parser as P

import regex
from dateparser.languages.loader import default_loader
from dateparser.timezone_parser import _tz_regexes
from htmldate.extractors import get_external_parser, month_words

SETTINGS = get_external_parser()._settings
LETTERS = set(string.ascii_lowercase)
NON_LETTERS = [chr(c) for c in range(0x20, 0x7F) if not chr(c).isalpha()]
CAP = 200_000
# st/nd/rd/th after a number are dropped by Dictionary._split_by_known_words in every locale
ALWAYS = {"st", "nd", "rd", "th", *SETTINGS.SKIP_TOKENS}


class TooBig(Exception):
    pass


@cache
def ascii_letters_matching(ch, ignorecase):
    """ASCII letters (lowercase) a literal char matches, by the regex module's own case-insensitive rules."""
    flags = regex.IGNORECASE if ignorecase else 0
    pat = regex.compile(regex.escape(ch), flags)
    return {c.lower() for c in string.ascii_letters if pat.fullmatch(c)}


def class_options(items, ignorecase):
    """Abstract symbols for one character of an sre class: letters (or `?`), `#` for a non-letter."""
    neg = any(op is C.NEGATE for op, _ in items)

    def member(ch):
        for op, av in items:
            if op is C.LITERAL and chr(av) == ch:
                return True
            if op is C.RANGE and av[0] <= ord(ch) <= av[1]:
                return True
            if op is C.CATEGORY:
                cat = {C.CATEGORY_DIGIT: ch.isdigit(), C.CATEGORY_NOT_DIGIT: not ch.isdigit(),
                       C.CATEGORY_SPACE: ch.isspace(), C.CATEGORY_NOT_SPACE: not ch.isspace(),
                       C.CATEGORY_WORD: ch.isalnum() or ch == "_",
                       C.CATEGORY_NOT_WORD: not (ch.isalnum() or ch == "_")}[av]
                if cat:
                    return True
        return False

    def matches(ch):
        variants = {ch, ch.lower(), ch.upper()} if ignorecase else {ch}
        if ignorecase and ch in "sS":
            variants.add("ſ")
        if ignorecase and ch in "kK":
            variants.add("K")
        return any(member(v) for v in variants) != neg

    letters = {c for c in LETTERS if matches(c)}
    out = set()
    if letters:
        out |= letters if len(letters) <= 2 else {"?"}
    if any(matches(c) for c in NON_LETTERS):
        out.add("#")
    return out


def norm(s):
    while "##" in s:
        s = s.replace("##", "#")
    while "**" in s:
        s = s.replace("**", "*")
    return s


def concat(a, b):
    if len(a) * len(b) > CAP:
        raise TooBig
    return {norm(x + y) for x in a for y in b}


def lang(items, ic):
    out = {""}
    for op, av in items:
        out = concat(out, node(op, av, ic))
    return out


def node(op, av, ic):
    if op is C.LITERAL:
        letters = ascii_letters_matching(chr(av), ic)
        if letters:
            return letters
        return {"#"} if chr(av).isascii() and not chr(av).isalpha() else set()
    if op is C.NOT_LITERAL or op is C.ANY:
        return {"?", "#"}
    if op is C.IN:
        return class_options(av, ic)
    if op is C.BRANCH:
        return set().union(*(lang(alt, ic) for alt in av[1]))
    if op is C.SUBPATTERN:
        _, add, rem, sub = av
        return lang(sub, (ic or bool(add & C.SRE_FLAG_IGNORECASE)) and not rem & C.SRE_FLAG_IGNORECASE)
    if op is C.GROUPREF:
        return {"*", "*#*", "#", "*#", "#*", ""}
    if op is C.ATOMIC_GROUP:
        return lang(av, ic)
    if op in (C.MAX_REPEAT, C.MIN_REPEAT, C.POSSESSIVE_REPEAT):
        lo, hi, sub = av
        body = lang(sub, ic)
        if not any(set(s) - {"#"} for s in body):
            out = {""} if lo == 0 or "" in body else set()
            return out | ({"#"} if "#" in "".join(body) and (hi is C.MAXREPEAT or hi > 0) else set())
        if hi is C.MAXREPEAT or hi > 4:
            return {"*", "*#*", "#", "*#", "#*", ""}
        out, cur = set(), {""}
        for k in range(hi + 1):
            if k >= lo:
                out |= cur
            cur = concat(cur, body)
        return out
    if op in (C.ASSERT, C.ASSERT_NOT, C.AT):
        return {""}
    raise TooBig(f"{op}")


def regex_runs(pattern):
    """Maximal letter runs of the ASCII strings `pattern` (a `regex`-module pattern object) can match."""
    tree = P.parse(pattern.pattern, C.SRE_FLAG_IGNORECASE if pattern.flags & regex.IGNORECASE else 0)
    runs = set()
    for s in lang(tree, bool(pattern.flags & regex.IGNORECASE)):
        runs.update(r for r in s.split("#") if r)
    return runs


def key_runs(key):
    if not key.isascii():
        return set()
    return {r for r in "".join(c if c in LETTERS else "#" for c in key).split("#") if r}


def locale_line(loc):
    native = loc._native
    nws = str(native.info.get("no_word_spacing", False)).strip().lower() in ("true", "1", "yes")
    d = native._get_dictionary(SETTINGS)
    words = set(ALWAYS)
    for key in d:
        words |= key_runs(key)
    try:
        words |= regex_runs(d._get_match_relative_regex_cache())
        simplified = set()
        for simplification in native._get_simplifications(SETTINGS):
            simplified |= regex_runs(next(iter(simplification)))
    except TooBig:
        return f"L {loc.shortname} any"
    if nws and simplified:
        # unanchored rewrites could splice their output into the middle of a word
        return f"L {loc.shortname} any"
    words |= simplified
    if any(set(w) & {"?", "*"} for w in words):
        return f"L {loc.shortname} any"
    return f"L {loc.shortname} {'pieces' if nws else 'words'} {' '.join(sorted(words))}"


def tz_words():
    names = _tz_regexes().names.pattern.split("|")
    words = set()
    for name in names:
        words |= {r.lower() for r in regex.findall(r"[^\W\d_]+", name)}
    return sorted(words)


def main():
    print(f"# Generated by harness/gen_dateparser_tables.py from dateparser {version('dateparser')}, "
          f"htmldate {version('htmldate')}. Do not edit.")
    for loc in default_loader.get_locales():
        print(locale_line(loc))
    print("T " + " ".join(tz_words()))
    print("M " + " ".join(sorted(month_words())))


if __name__ == "__main__":
    sys.exit(main())
