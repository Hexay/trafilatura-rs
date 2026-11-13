"""Locale data and patterns of the native dateparser port (crates/trafilatura-rs/src/dates/dateparser/locales.rs).
usage: gen_dateparser_locales.py > crates/trafilatura-rs/src/dates/dateparser/locales.txt

Everything comes from the installed dateparser, with htmldate's parser settings: each locale's normalized
dictionary, simplifications and relative-time patterns, the timezone and part-of-day patterns. Patterns are in
fancy-regex syntax (harness/dp_regex.py); a long character class is written once as a `K` line and referenced as
`@<n>@`, a pattern once as a `P` line and referenced by number. Tab-separated lines:
  V <dateparser> <regex> <unidata>
  K <n> <class>  /  P <n> <pattern>
  D <n>                               starts a dictionary, made of the following lines:
  W <key> [<value>]                   entries, in iteration order (no value: a skip word)
  A <key> <meaning>...                Dictionary._get_meanings, where there are several
  C <token> <weekday>                 Dictionary._weekday_month_conflicts
  M <token>...                        Dictionary._month_translations
  L <i> <shortname> <name> <date_order> <no_word_spacing 0|1> <letter variants 0|1> <dictionary>   a locale, with:
  U <reason>                          it cannot be ported: strings it may apply to stay on Python
  S <pattern> <replacement>           simplifications, in order
  R <pattern>  /  Q <pattern>         the split / match relative-time regexes
  T <pattern> <replacement> <merged>  relative translations, in order (merged: \\1 is whichever group matched)
  O <locale> <native>                 the candidate locales in loader order
  Z <pattern> <reverse>               a timezone pattern family, then its
  z <name> <offset seconds> <pattern>   timezones, one per group tz<k>, each alone in the family's template
  G <name> <pattern>                  other patterns
  Y <from> <to>                       letter variants (ar, fa)"""
import sys
import unicodedata
from importlib.metadata import version

import dp_regex
import regex
from dateparser import _parts_of_day as pod
from dateparser.languages import locale as dp_locale
from dateparser.languages.loader import default_loader
from dateparser.timezone_parser import _tz_regexes
from dateparser.timezones import timezone_info_list
from htmldate.extractors import get_external_parser

SETTINGS = get_external_parser()._settings
PATTERNS = {}
DICTIONARIES = {}


def plain(text):
    if "\t" in text or "\n" in text:
        raise dp_regex.Unsupported(f"tab or newline in {text!r}")
    return text


def pattern(p):
    """(pool number, merged) of a compiled `regex` pattern."""
    src, merged = dp_regex.translate(p)
    return PATTERNS.setdefault(plain(src), len(PATTERNS)), merged


def dictionary(d):
    lines = []
    for key in d:
        value = d[key]
        lines.append("\t".join(["W", plain(key)] + ([] if value is None else [plain(value)])))
        meanings = d._get_meanings(key)
        if len(meanings) > 1:
            lines.append("\t".join(["A", key, *map(plain, meanings)]))
    lines += ["\t".join(["C", plain(k), v]) for k, v in d._weekday_month_conflicts.items()]
    lines.append("\t".join(["M", *map(plain, sorted(d._month_translations))]))
    return DICTIONARIES.setdefault(tuple(lines), len(DICTIONARIES))


def locale_block(i, loc):
    info = loc.info
    head = ["L", str(i), loc.shortname, info["name"], info.get("date_order", "-"),
            str(int(dp_locale._parse_bool(info.get("no_word_spacing", False)))), str(int(loc._translates_letters))]
    try:
        d = loc._get_dictionary(SETTINGS)
        lines = ["\t".join([*head, str(dictionary(d))])]
        for simplification in loc._get_simplifications(SETTINGS):
            (p, replacement), = simplification.items()
            # _apply_simplifications lowercases the whole string after each substitution
            lower = replacement.lower()
            if lower != replacement and ("\\" in replacement or lower.lower() != lower):
                raise dp_regex.Unsupported("a simplification writes letters str.lower() changes")
            lines.append(f"S\t{pattern(p)[0]}\t{plain(lower)}")
        lines.append(f"R\t{pattern(d._get_split_relative_regex_cache())[0]}")
        lines.append(f"Q\t{pattern(d._get_match_relative_regex_cache())[0]}")
        for p, key in loc._get_relative_translations(SETTINGS).items():
            n, merged = pattern(p)
            lines.append(f"T\t{n}\t{plain(key)}\t{int(merged)}")
    except dp_regex.Unsupported as e:
        return ["\t".join([*head, "-"]), f"U\t{e}".replace("\n", " ")]
    return lines


def tz_lines():
    """Each family as compiled by timezone_parser._build_tz_regexes (checked), then each alternative alone in the
    family's template: a REVERSE family's match is that of its first alternative that matches."""
    families = iter(_tz_regexes().families)
    lines = []
    for tz_info in timezone_info_list:
        for template in tz_info["regex_patterns"]:
            p, timezones = next(families)
            variants = []
            for name, offset in tz_info["timezones"]:
                for variant in [name] + [regex.sub(a, b, name) for a, b in tz_info.get("replace", [])]:
                    variants.append((name, offset, variant))
            groups = "|".join(f"(?P<tz{k}>{v})" for k, (_, _, v) in enumerate(variants))
            # tz.rs reads the families with these templates only
            expected = r"(.)%s$" if p.flags & regex.REVERSE else r"(\W|\d|_)%s($|\W|(?=\d))"
            if template != expected or (not p.flags & regex.REVERSE and tz_info.get("replace")):
                raise SystemExit("timezone families are no longer built as this generator assumes")
            if p.pattern != template % f"(?:{groups})" or len(timezones) != len(variants):
                raise SystemExit("timezone families are no longer built as this generator assumes")
            flags = p.flags & ~regex.REVERSE
            lines.append(f"Z\t{pattern(regex.compile(p.pattern, flags))[0]}\t{int(bool(p.flags & regex.REVERSE))}")
            for name, offset, variant in variants:
                alone = pattern(regex.compile(template % f"(?:{variant})", flags))[0]
                lines.append(f"z\t{plain(name)}\t{offset}\t{alone}")
    return lines


def main():
    blocks, index = [], {}

    def block(loc):
        if id(loc) not in index:
            index[id(loc)] = len(index)
            blocks.extend(locale_block(index[id(loc)], loc))
        return index[id(loc)]

    order = [f"O\t{block(loc)}\t{block(loc._native)}" for loc in default_loader.get_locales()]
    rest = tz_lines()
    for name, p in (("part_of_day", pod._PART_OF_DAY_PATTERN), ("time", pod._TIME_PATTERN),
                    ("trailing_number", pod._TRAILING_NUMBER_PATTERN)):
        rest.append(f"G\t{name}\t{pattern(p)[0]}")
    rest += [f"Y\t{chr(a)}\t{chr(b) if isinstance(b, int) else b}" for a, b in sorted(dp_locale._LETTER_VARIANTS.items())]
    print("# Generated by harness/gen_dateparser_locales.py. Do not edit.")
    print(f"V\t{version('dateparser')}\t{regex.__version__}\t{unicodedata.unidata_version}")
    print(*(f"K\t{n}\t{src}" for src, n in dp_regex.CLASSES.items()), sep="\n")
    print(*(f"P\t{n}\t{src}" for src, n in PATTERNS.items()), sep="\n")
    for lines, n in DICTIONARIES.items():
        print(f"D\t{n}", *lines, sep="\n")
    print(*blocks, *order, *rest, sep="\n")


if __name__ == "__main__":
    sys.exit(main())
