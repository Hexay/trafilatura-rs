"""Tables for the native dateparser shortcut (see crates/trafilatura-rs/src/dates/dateparser/mod.rs).
usage: gen_dateparser_tables.py > crates/trafilatura-rs/src/dates/dateparser/tables.txt

Per locale, the words (maximal runs of `[^\\W\\d_]` letters, folded as in gen_dateparser_chars.py) that may survive
in a date string the locale accepts: the runs of its dictionary keys and of every string its relative-time and
simplification regexes can match (the language is over-approximated: lookarounds are dropped, wide classes become
wildcards). Lines:
  L <locale> words <word>...           a surviving word must be one of these
  L <locale> pieces <word>...          no_word_spacing: a surviving word must be a concatenation of these
  L <locale> any                       nothing can be excluded (a wildcard reached the word list)
  G <letter>...                        after an L line: rewrites (simplifications) that may splice words or copy
                                       wildcards apply only to strings with one of these letters (* = any string)...
  C <letter>... | C any                ...in which case every letter must be one of these instead
  X <locale> <mode> ...                as L, for the letter-variant twin of ar/fa, which first applies Y to letters
  Y <from> <to>...                     locale._LETTER_VARIANTS
  T <word>...                          timezone names (pop_tz_offset_from_string may remove them)
  M <word>...                          htmldate's month_words() (has_date_cue)"""
import sys
from importlib.metadata import version
from re import _constants as C
from re import _parser as P

import regex
from dateparser.languages import locale as dp_locale
from dateparser.languages.loader import default_loader
from dateparser.timezone_parser import _tz_regexes
from gen_dateparser_chars import DIGIT, SPACE, fold, is_letter
from htmldate.extractors import get_external_parser, month_words

SETTINGS = get_external_parser()._settings
CAP = 200_000
WIDE = 8
WILD = {"*", "*#*", "#", "*#", "#*", ""}
# `?`: any letter, `*`: any string, `=`: any letters a rewrite copies to its output
WILDCARDS = {"?", "*", "="}
# st/nd/rd/th after a number are dropped by Dictionary._split_by_known_words in every locale
ALWAYS = {fold(w) for w in ("st", "nd", "rd", "th", *SETTINGS.SKIP_TOKENS)}
LETTER_CATEGORIES = {C.CATEGORY_WORD, C.CATEGORY_NOT_DIGIT, C.CATEGORY_NOT_SPACE}


class TooBig(Exception):
    pass


def symbol(ch):
    return fold(ch) if is_letter(ch) else "#"


def class_options(items):
    """Abstract symbols for one character of an sre class: folded letters (or `?`), `#` for a non-letter."""
    if any(op is C.NEGATE for op, _ in items):
        return {"?", "#"}
    out, letters = set(), set()
    for op, av in items:
        if op is C.LITERAL:
            chars = [chr(av)]
        elif op is C.RANGE:
            if av[1] - av[0] > 0x3000:
                return {"?", "#"}
            chars = [chr(c) for c in range(av[0], av[1] + 1) if not 0xD800 <= c <= 0xDFFF]
        elif op is C.CATEGORY:
            if av in LETTER_CATEGORIES:
                out.add("?")
            out.add("#")
            continue
        else:
            raise TooBig(f"class item {op}")
        for ch in chars:
            (letters if is_letter(ch) else out).add(symbol(ch))
    out |= letters if len(letters) <= WIDE else {"?"}
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


def kept(abstract, inside):
    """Inside a group the replacement copies, a wildcard letter is `=`: it survives the rewrite."""
    return {s.replace("?", "=").replace("*", "=") for s in abstract} if inside else abstract


def lang(items, keep, inside):
    out = {""}
    for op, av in items:
        out = concat(out, node(op, av, keep, inside))
    return out


def node(op, av, keep, inside):
    if op is C.LITERAL:
        return {symbol(chr(av))}
    if op is C.NOT_LITERAL or op is C.ANY:
        return kept({"?", "#"}, inside)
    if op is C.IN:
        return kept(class_options(av), inside)
    if op is C.BRANCH:
        return set().union(*(lang(alt, keep, inside) for alt in av[1]))
    if op is C.SUBPATTERN:
        return lang(av[3], keep, inside or av[0] in keep)
    if op is C.GROUPREF:
        return WILD
    if op is C.ATOMIC_GROUP:
        return lang(av, keep, inside)
    if op in (C.MAX_REPEAT, C.MIN_REPEAT, C.POSSESSIVE_REPEAT):
        lo, hi, sub = av
        body = lang(sub, keep, inside)
        if not any(set(s) - {"#"} for s in body):
            out = {""} if lo == 0 or "" in body else set()
            return out | ({"#"} if "#" in "".join(body) and (hi is C.MAXREPEAT or hi > 0) else set())
        if hi is C.MAXREPEAT or hi > 4:
            return kept(WILD, inside)
        out, cur = set(), {""}
        for k in range(hi + 1):
            if k >= lo:
                out |= cur
            cur = concat(cur, body)
        return out
    if op in (C.ASSERT, C.ASSERT_NOT, C.AT):
        return {""}
    raise TooBig(f"{op}")


def strings(pattern, replacement=None):
    """Abstract strings (folded letters, `#`, wildcards) `pattern` (a `regex`-module pattern object) can match; with
    the `replacement` of a rewrite, wildcards in the groups it copies are `=`."""
    keep = set()
    for num, name in regex.findall(r"\\(\d+)|\\g<(\w+)>", replacement or ""):
        keep.add(int(num) if num else pattern.groupindex.get(name, int(name) if name.isdigit() else -1))
    tree = P.parse(pattern.pattern, C.SRE_FLAG_IGNORECASE if pattern.flags & regex.IGNORECASE else 0)
    return lang(tree, keep, False)


def runs(abstract):
    return {r for s in abstract for r in s.split("#") if r}


def locale_line(kind, loc):
    """`kind` line for the locale whose applicability `loc.is_applicable` checks."""
    nws = dp_locale._parse_bool(loc.info.get("no_word_spacing", False))
    d = loc._get_dictionary(SETTINGS)
    words = set(ALWAYS)
    for key in d:
        words |= runs({fold(key)})
    try:
        words |= runs(strings(d._get_match_relative_regex_cache()))
        rewrites = [strings(*next(iter(s.items()))) for s in loc._get_simplifications(SETTINGS)]
    except TooBig:
        return [f"{kind} {loc.shortname} any"]
    if any(set(w) & WILDCARDS for w in words):
        return [f"{kind} {loc.shortname} any"]
    letters = set("".join(words))
    guard, risky = set(), False
    for abstract in rewrites:
        letters |= set("".join(abstract)) - {"#"}
        if not nws and not any(set(s) & WILDCARDS for s in abstract):
            words |= runs(abstract)
            continue
        # Unanchored rewrites may splice their output into a word and copied wildcards make any word possible,
        # but neither removes a letter it neither consumes nor copies, and neither applies without its letters.
        risky |= any(set(s) & {"?", "*"} for s in abstract)
        for s in abstract:
            literal = set(s) - {"#"} - WILDCARDS
            guard |= literal if literal else {"*"}
    lines = [f"{kind} {loc.shortname} {'pieces' if nws else 'words'} {' '.join(sorted(words))}"]
    if guard:
        lines.append(f"G {' '.join(sorted(guard))}")
        lines.append("C any" if risky else f"C {' '.join(sorted(letters - WILDCARDS))}")
    return lines


def tz_words():
    return sorted(runs({fold(name) for name in _tz_regexes().names.pattern.split("|")}))


def check_assumptions():
    """What dateparser.date.sanitize_date and the tables rely on: chars.rs mirrors these classes."""
    if any(DIGIT.fullmatch(c) or is_letter(c) for c in "#?*") or SPACE.fullmatch("_"):
        raise SystemExit("unexpected class membership")


def main():
    check_assumptions()
    print(f"# Generated by harness/gen_dateparser_tables.py from dateparser {version('dateparser')}, "
          f"htmldate {version('htmldate')}. Do not edit.")
    for loc in default_loader.get_locales():
        print(*locale_line("L", loc._native), sep="\n")
        if loc._native is not loc:
            print(*locale_line("X", loc), sep="\n")
    variants = dp_locale._LETTER_VARIANTS
    print("Y " + " ".join(f"{a:x} {b:x}" for a, b in sorted(variants.items())))
    print("T " + " ".join(tz_words()))
    print("M " + " ".join(sorted(month_words())))


if __name__ == "__main__":
    sys.exit(main())
