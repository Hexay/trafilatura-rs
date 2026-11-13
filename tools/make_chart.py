"""README benchmark chart from tools/bench.sh reports, so bar lengths and labels come from the same numbers.
usage: make_chart.py collect DATA.json VERSION REPORT...   median over runs of each engine's mean CPU ms/doc, from saved
                                                           bench.sh stdout (VERSION: 1.11.0 or 2.3.1), merged into DATA
       make_chart.py svg DATA.json > docs/assets/extract-speed.svg
Colors: accent and de-emphasis gray checked with the dataviz validator against #fcfcfb/#ffffff and #1a1a19/#0d1117."""
import json
import re
import statistics
import sys
from html import escape
from pathlib import Path

ENGINES = ("trafilatura", "trafilatura_rs", "rs")
ROW = re.compile(r"^(trafilatura|trafilatura_rs|rs)\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)\s+[\d.]+\s+([\d.]+|nan)$")
EXACT = re.compile(r"^(trafilatura_rs|rs) vs trafilatura: exact=(\d+)/(\d+)")

WIDTH, BAR_X, BAR_MAX = 720, 132, 340
BAR_H, PITCH, GROUP_GAP, TOP = 20, 28, 14, 78
FONT = "system-ui,-apple-system,'Segoe UI',Helvetica,Arial,sans-serif"
LIGHT = {"ink": "#0b0b0b", "ink2": "#52514e", "base": "#c3c2b7", "accent": "#2a78d6", "gray": "#8f8d87"}
DARK = {"ink": "#f0efec", "ink2": "#c3c2b7", "base": "#383835", "accent": "#3987e5", "gray": "#6a6964"}


def colors(c):
    return (f".title,.group,.label{{fill:{c['ink']}}}text{{fill:{c['ink2']}}}.ours{{fill:{c['accent']}}}"
            f".other{{fill:{c['gray']}}}.base{{stroke:{c['base']}}}")


# plain classes, no var(): some SVG renderers (and image proxies) don't do custom properties
STYLE = f"""
text{{font-family:{FONT};font-size:13px}}.title{{font-size:15px;font-weight:600}}.group{{font-weight:600}}
.value{{font-variant-numeric:tabular-nums}}.base{{stroke-width:1}}
{colors(LIGHT)}
@media (prefers-color-scheme:dark){{{colors(DARK)}}}
"""


def parse_report(path):
    """{engine: (mean, p50, p99, Minstr/doc)} and {engine: (exact, docs)} from one bench_report.py output."""
    rows, exact = {}, {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if m := ROW.match(line.strip()):
            rows[m[1]] = tuple(float(x) for x in m.group(2, 3, 4, 5))
        elif m := EXACT.match(line.strip()):
            exact[m[1]] = (int(m[2]), int(m[3]))
    if "trafilatura" not in rows or "trafilatura_rs" not in rows:
        raise SystemExit(f"{path}: no bench_report table")
    return rows, exact


def collect(data_path, version, reports):
    parsed = [parse_report(p) for p in reports]
    out = {"runs": len(parsed)}
    for engine in ENGINES:
        stats = [rows[engine] for rows, _ in parsed if engine in rows]
        if not stats:
            continue
        out[engine] = {key: round(statistics.median(s[i] for s in stats), 3)
                       for i, key in enumerate(("mean_ms", "p50_ms", "p99_ms", "minstr"))}
        out[engine]["mean_ms_runs"] = [s[0] for s in stats]
    for engine in ("trafilatura_rs", "rs"):
        exact = [ex[engine] for _, ex in parsed if engine in ex]
        if exact:
            out[engine]["exact"], out["docs"] = min(exact)
    path = Path(data_path)
    data = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    if data.setdefault("docs", out["docs"]) != out["docs"]:
        raise SystemExit(f"{version}: {out['docs']} docs, other versions {data['docs']}")
    data.setdefault("versions", {})[version] = out
    path.write_text(json.dumps(data, indent=1) + "\n", encoding="utf-8")


def bar(x, y, length, cls, tip):
    """Horizontal bar with a 4px rounded data end and a square baseline end; drawn length == `length`."""
    r = min(4.0, length / 2)
    body = length - r
    return (f'<path class="{cls}" d="M{x},{y}h{body:.1f}a{r},{r} 0 0 1 {r},{r}v{BAR_H - 2 * r}a{r},{r} 0 0 1 '
            f'-{r},{r}h-{body:.1f}z"><title>{escape(tip)}</title></path>')


def rows_for(data):
    """[(group heading, [(label, ms, accent, value text)])] in display order."""
    groups = []
    for version in ("1.11.0", "2.3.1"):
        v = data["versions"].get(version)
        if not v:
            continue
        py, rs, exact = v["trafilatura"]["mean_ms"], v["trafilatura_rs"]["mean_ms"], v["trafilatura_rs"]["exact"]
        same = "identical output" if exact == data["docs"] else f"same output on {exact:,} of {data['docs']:,}"
        groups.append((f"trafilatura {version}", [
            ("Python", py, False, f"{py:.2f} ms"),
            ("trafilatura-rs", rs, True, f"{rs:.2f} ms, {py / rs:.1f}x faster, {same}"),
        ]))
    other = data["versions"].get("1.11.0", {}).get("rs")
    if other:
        same = other["exact"]
        groups.append((f"rs-trafilatura, an independent extractor: same output as trafilatura 1.11.0 on {same:,} of "
                       f"{data['docs']:,} pages", [
                           (f"rs-trafilatura {data['rs_trafilatura']}", other["mean_ms"], False,
                            f"{other['mean_ms']:.2f} ms"),
                       ]))
    return groups


def svg(data):
    groups = rows_for(data)
    longest = max(ms for _, rows in groups for _, ms, _, _ in rows)
    scale = BAR_MAX / longest
    parts, y = [], TOP
    for heading, rows in groups:
        parts.append(f'<text class="group" x="0" y="{y + 14}">{escape(heading)}</text>')
        y += 22
        top = y
        for label, ms, accent, value in rows:
            length = ms * scale
            parts.append(f'<text class="label" x="{BAR_X - 10}" y="{y + 14.5}" '
                         f'text-anchor="end">{escape(label)}</text>')
            parts.append(bar(BAR_X, y, length, "ours" if accent else "other", f"{label}: {ms:.2f} ms per page"))
            parts.append(f'<text class="value" x="{BAR_X + length + 8:.1f}" y="{y + 14.5}">{escape(value)}</text>')
            y += PITCH
        parts.append(f'<line class="base" x1="{BAR_X}" y1="{top - 4}" x2="{BAR_X}" y2="{y - PITCH + BAR_H + 4}"/>')
        y += GROUP_GAP
    height = y - GROUP_GAP + 8
    title = f"trafilatura.extract() on {data['docs']:,} Common Crawl pages"
    runs = min(v["runs"] for v in data["versions"].values())
    subtitle = f"Mean CPU time per page, single thread, median of {runs} runs. Lower is better."
    alt = "; ".join(f"{h}: " + ", ".join(f"{label} {ms:.2f} ms" for label, ms, _, _ in rows) for h, rows in groups)
    return "\n".join([
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" '
        f'role="img" aria-label="{escape(title)}. {escape(alt)}">',
        f"<style>{STYLE}</style>",
        f'<text class="title" x="0" y="20">{escape(title)}</text>',
        f'<text x="0" y="42">{escape(subtitle)}</text>',
        f'<text x="0" y="60">{escape(data["call"])}</text>',
        *parts, "</svg>", ""])


def main():
    cmd, data_path, *rest = sys.argv[1:]
    if cmd == "collect":
        collect(data_path, rest[0], rest[1:])
    elif cmd == "svg":
        sys.stdout.write(svg(json.loads(Path(data_path).read_text(encoding="utf-8"))))
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
