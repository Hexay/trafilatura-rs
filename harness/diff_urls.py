"""The native URL resolution (py/urllib.rs) against the running interpreter's urllib.parse, on the corpus' links.
  diff_urls.py collect docs.jsonl pairs.jsonl     (base, href) for courlan.fix_relative_urls and (page url, src) for
                                                   urljoin, from every a/@href and img src of the corpus (.venv)
  diff_urls.py check pairs.jsonl native.jsonl     compare `harness urls pairs.jsonl native.jsonl` output with this
                                                   interpreter's results; run it under every supported Python
check needs nothing but the standard library: courlan's two helpers are inlined."""
import json
import sys
from html import unescape
from urllib.parse import urljoin, urlsplit, urlunsplit


def get_base_url(url):
    parsed = urlsplit(unescape(url))
    return (parsed.scheme + "://" if parsed.scheme else "") + parsed.netloc


def fix_relative_urls(baseurl, url):
    if url.startswith("{"):
        return url
    base_netloc = urlsplit(baseurl).netloc
    split_url = urlsplit(url)
    if split_url.netloc not in (base_netloc, ""):
        if split_url.scheme:
            return url
        return urlunsplit(split_url._replace(scheme="http"))
    return urljoin(baseurl, url)


def expected(kind, base, url):
    try:
        if kind == "base":
            return get_base_url(base)
        return fix_relative_urls(base, url) if kind == "fix" else urljoin(base, url)
    except ValueError:
        return "EXC:ValueError"


def collect(docs_path, out_path):
    from lxml.html import fromstring

    seen = set()
    with open(docs_path, encoding="utf-8") as f, open(out_path, "w", encoding="utf-8") as out:
        for line in f:
            doc = json.loads(line)
            url = doc["url"]
            try:
                base = get_base_url(url)
                tree = fromstring(doc["html"])
            except Exception:  # noqa: BLE001
                continue
            pairs = [("base", url, "")]
            # smart strings would keep every parsed tree alive through `seen`
            pairs += [("fix", base, href) for href in tree.xpath("//a/@href", smart_strings=False)]
            pairs += [("join", url, src) for src in tree.xpath("//img/@src | //img/@data-src", smart_strings=False)]
            for pair in pairs:
                if pair not in seen:
                    seen.add(pair)
                    out.write(json.dumps(pair) + "\n")
    print(f"{len(seen)} pairs")


def check(pairs_path, native_path):
    covered = mismatched = 0
    with open(pairs_path, encoding="utf-8") as p, open(native_path, encoding="utf-8") as n:
        for pair_line, native_line in zip(p, n):
            kind, base, url = json.loads(pair_line)
            native = json.loads(native_line)
            if native is None:
                continue
            covered += 1
            want = expected(kind, base, url)
            if native != want:
                mismatched += 1
                if mismatched <= 20:
                    print(f"{kind} {base!r} {url!r}: native {native!r}, Python {want!r}")
    print(f"Python {sys.version.split()[0]}: {covered} covered natively, {mismatched} differ")


if __name__ == "__main__":
    {"collect": collect, "check": check}[sys.argv[1]](*sys.argv[2:])
