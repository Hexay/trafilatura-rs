"""Translate live Python `re` patterns into fancy-regex syntax with Python's exact semantics.
usage: gen_py_regexes.py TITLE SPEC... > crates/trafilatura-rs/src/<module>/regexes.rs
  SPEC = RUSTNAME=module:ATTR      a compiled pattern object (or pattern string) in an importable module
       | RUSTNAME=@<flags>:<pattern>  an inline pattern; flags are letters of re flags ("I", "S", ...) or empty
The sre parse tree is re-emitted node by node. Every character class, `.` and case-insensitive literal is expanded
to explicit code point ranges by running Python's own `re` over all code points, so Unicode `\\w`/`\\d`/`\\s` and
`re.I` equivalences (e.g. `ı`/`İ`/`i`) match exactly what Python matches. `\\b` and `$` become lookarounds."""
import importlib
import re
import sys
import unicodedata
from functools import lru_cache
from re import _constants as C
from re import _parser as P

ALL = "".join(chr(c) for c in range(sys.maxunicode + 1) if not 0xD800 <= c <= 0xDFFF)
CATEGORIES = {C.CATEGORY_DIGIT: r"\d", C.CATEGORY_NOT_DIGIT: r"\D", C.CATEGORY_SPACE: r"\s",
              C.CATEGORY_NOT_SPACE: r"\S", C.CATEGORY_WORD: r"\w", C.CATEGORY_NOT_WORD: r"\W"}
SAFE = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_")


def esc(cp):
    return chr(cp) if chr(cp) in SAFE else f"\\x{{{cp:x}}}"


@lru_cache(maxsize=None)
def code_points(py_class, flags):
    """Sorted (lo, hi) ranges of the code points Python's `re` matches with this one-character pattern."""
    hits = [m.start() for m in re.finditer(py_class, ALL, flags)]
    cps = [ord(ALL[i]) for i in hits]
    ranges = []
    for cp in cps:
        if ranges and (ranges[-1][1] == cp - 1 or (ranges[-1][1], cp) == (0xD7FF, 0xE000)):
            ranges[-1][1] = cp
        else:
            ranges.append([cp, cp])
    return tuple(map(tuple, ranges))


def rs_class(py_class, flags):
    ranges = code_points(py_class, flags)
    assert ranges, f"empty class {py_class!r}"
    if ranges == ((0, sys.maxunicode),):
        return "(?s:.)"
    if len(ranges) == 1 and ranges[0][0] == ranges[0][1]:
        return esc(ranges[0][0])
    return "[" + "".join(esc(a) if a == b else f"{esc(a)}-{esc(b)}" for a, b in ranges) + "]"


def py_set(items):
    out, neg = [], ""
    for op, av in items:
        if op is C.NEGATE:
            neg = "^"
        elif op is C.LITERAL:
            out.append(f"\\U{av:08x}")
        elif op is C.RANGE:
            out.append(f"\\U{av[0]:08x}-\\U{av[1]:08x}")
        elif op is C.CATEGORY:
            out.append(CATEGORIES[av])
        else:
            raise NotImplementedError(op)
    return f"[{neg}{''.join(out)}]"


def char_flags(flags):
    return flags & (re.I | re.A | re.S)


class Emitter:
    def __init__(self, names):
        self.names = names  # group id -> name

    def word(self, flags):
        return rs_class(r"\w", char_flags(flags))

    def seq(self, sub, flags):
        return "".join(self.node(op, av, flags) for op, av in sub)

    def node(self, op, av, flags):
        if op is C.LITERAL:
            return rs_class(f"\\U{av:08x}", char_flags(flags)) if flags & re.I else esc(av)
        if op is C.NOT_LITERAL:
            return rs_class(f"[^\\U{av:08x}]", char_flags(flags))
        if op is C.ANY:
            return rs_class(".", char_flags(flags))
        if op is C.IN:
            return rs_class(py_set(av), char_flags(flags))
        if op is C.BRANCH:
            return "(?:" + "|".join(self.seq(alt, flags) for alt in av[1]) + ")"
        if op is C.SUBPATTERN:
            gid, add, rem, sub = av
            inner = self.seq(sub, (flags | add) & ~rem)
            if gid is None:
                return f"(?:{inner})"
            name = self.names.get(gid)
            return f"(?P<{name}>{inner})" if name else f"({inner})"
        if op in (C.MAX_REPEAT, C.MIN_REPEAT):
            lo, hi, sub = av
            body = f"(?:{self.seq(sub, flags)})"
            rep = {(0, C.MAXREPEAT): "*", (1, C.MAXREPEAT): "+", (0, 1): "?"}.get((lo, hi))
            if rep is None:
                rep = f"{{{lo},}}" if hi is C.MAXREPEAT else f"{{{lo},{hi}}}"
            return body + rep + ("?" if op is C.MIN_REPEAT else "")
        if op in (C.ASSERT, C.ASSERT_NOT):
            direction, sub = av
            kind = ("=" if op is C.ASSERT else "!") if direction == 1 else ("<=" if op is C.ASSERT else "<!")
            return f"(?{kind}{self.seq(sub, flags)})"
        if op is C.AT:
            w = self.word(flags)
            return {
                C.AT_BEGINNING: "(?m:^)" if flags & re.M else r"\A",
                C.AT_BEGINNING_STRING: r"\A",
                C.AT_END: "(?m:$)" if flags & re.M else r"(?=\n?\z)",
                C.AT_END_STRING: r"\z",
                C.AT_BOUNDARY: f"(?:(?<={w})(?!{w})|(?<!{w})(?={w}))",
                C.AT_NON_BOUNDARY: f"(?:(?<={w})(?={w})|(?<!{w})(?!{w}))",
            }[av]
        if op is C.GROUPREF:
            return f"\\{av}"
        raise NotImplementedError(f"{op} {av}")


def translate(pattern):
    assert isinstance(pattern.pattern, str)
    tree = P.parse(pattern.pattern, pattern.flags)
    names = {v: k for k, v in pattern.groupindex.items()}
    return Emitter(names).seq(tree, tree.state.flags)


def resolve(spec):
    name, _, src = spec.partition("=")
    if src.startswith("@"):
        flags, _, pat = src[1:].partition(":")
        return name, re.compile(pat, sum(getattr(re, f) for f in flags))
    module, _, attr = src.partition(":")
    obj = getattr(importlib.import_module(module), attr)
    return name, obj if isinstance(obj, re.Pattern) else re.compile(obj)


def rs_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main():
    title, specs = sys.argv[1], sys.argv[2:]
    print(f"//! Generated by harness/gen_py_regexes.py from Python {sys.version.split()[0]} (Unicode "
          f"{unicodedata.unidata_version}): {title}. Do not edit.")
    print("use crate::py::pyre::Pattern;")
    for spec in specs:
        name, pattern = resolve(spec)
        print(f"/// `{pattern.pattern[:100]}`".replace("\n", " ") + (" (re.I)" if pattern.flags & re.I else ""))
        print(f"pub static {name}: Pattern = Pattern::new({rs_str(translate(pattern))});")


if __name__ == "__main__":
    main()
