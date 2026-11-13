"""`regex`-module patterns (dateparser's) in fancy-regex syntax with the `regex` module's exact semantics.
The sre parse tree is re-emitted as gen_py_regexes.py does, but every class, `.` and case-insensitive literal is
expanded by running the `regex` module itself over all code points, and possessive repeats and atomic groups are
kept. `translate` raises Unsupported for syntax sre cannot parse; a locale using such a pattern stays on Python."""
import re
from functools import lru_cache
from re import _constants as C
from re import _parser as P

import gen_py_regexes as G
import regex
from regex import _regex

FLAGS = {regex.I: re.I, regex.M: re.M, regex.S: re.S, regex.X: re.X}
DUPLICATE_N = "(?P<n>"


class Unsupported(Exception):
    pass


@lru_cache(maxsize=None)
def code_points(py_class, flags):
    """As gen_py_regexes.code_points, but matched by the `regex` module."""
    rflags = sum(r for r, p in FLAGS.items() if flags & p)
    cps = [ord(m.group()) for m in regex.finditer(py_class, G.ALL, rflags)]
    ranges = []
    for cp in cps:
        if ranges and (ranges[-1][1] == cp - 1 or (ranges[-1][1], cp) == (0xD7FF, 0xE000)):
            ranges[-1][1] = cp
        else:
            ranges.append([cp, cp])
    return tuple(map(tuple, ranges))


G.code_points = code_points
META = set("\\.+*?()|[]{}^$#&-~")


def esc(cp):
    """A code point as a literal in fancy-regex syntax, inside or outside a class."""
    ch = chr(cp)
    if ch in META:
        return "\\" + ch
    if ch != "@" and (0x20 <= cp < 0x7F or (ch.isalnum() and ch.isprintable())):
        return ch
    return f"\\x{{{cp:x}}}"


G.esc = esc
CLASSES = {}
_rs_class = G.rs_class


def intern(src):
    """A long class as `@<n>@`, its source being key n of CLASSES."""
    return src if len(src) <= 8 else f"@{CLASSES.setdefault(src, len(CLASSES))}@"


G.rs_class = lambda py_class, flags: intern(_rs_class(py_class, flags))


def literal(cp, flags):
    if not flags & re.I:
        return G.esc(cp)
    cases = sorted(set(_regex.get_all_cases(int(regex.I | regex.U), cp)) | {cp})
    if len(cases) == 1:
        return G.esc(cp)
    return intern("[" + "".join(G.esc(c) for c in cases) + "]")


class Emitter(G.Emitter):
    def node(self, op, av, flags):
        if op is C.LITERAL:
            return literal(av, flags)
        if op is C.POSSESSIVE_REPEAT:
            return f"(?>{super().node(C.MAX_REPEAT, av, flags)})"
        if op is C.ATOMIC_GROUP:
            return f"(?>{self.seq(av, flags)})"
        return super().node(op, av, flags)


def translate(pattern):
    """(fancy-regex source, merged): `merged` when the pattern reuses the group name `n` (dateparser's relative
    translations), which the `regex` module numbers as one group; here each use is its own group."""
    src = pattern.pattern
    uses = src.count(DUPLICATE_N)
    merged = uses > 1
    if merged:
        src = src.replace(DUPLICATE_N, "(")
        if regex.compile(src, pattern.flags).groups != uses:
            raise Unsupported("groups besides n")
    flags = sum(p for r, p in FLAGS.items() if pattern.flags & r)
    try:
        tree = P.parse(src, flags)
    except Exception as e:  # noqa: BLE001 - any sre parse error means unsupported
        raise Unsupported(str(e)) from e
    names = {v: k for k, v in regex.compile(src, pattern.flags).groupindex.items()}
    try:
        return Emitter(names).seq(tree, tree.state.flags), merged
    except NotImplementedError as e:
        raise Unsupported(str(e)) from e
