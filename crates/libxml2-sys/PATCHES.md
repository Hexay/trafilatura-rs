# Local changes to the vendored libxml2 2.14.6

## Build configuration (build.rs) vs lxml 6.1.3

lxml's wheels (`etree.LIBXML_FEATURES`: html, xpath, regexp, iconv, schematron, xmlschema, catalog, zlib) build
libxml2 with CMake/autotools defaults plus static GNU libiconv and zlib, threads on. trafex keeps every default
that in-memory HTML parsing, XPath or serialization can observe (HTML, XPath, output, regexps, SAX1, push, threads,
ISO8859X) and differs only here:

| option | lxml | trafex | why it cannot change output |
|---|---|---|---|
| zlib, lzma | zlib on | off | only decompress files/URLs libxml2 opens itself; trafex parses memory buffers |
| catalog | on | off | resolves external DTD/entity URIs, which the HTML parser never loads (NONET) |
| debug, modules, http, programs, tests, python | mixed | off | not reachable from trafex |
| iconv | static GNU libiconv | system iconv (glibc on Linux, libiconv on macOS), vendored win-iconv 0.0.11 on Windows (`win-iconv/`, public domain) | see below |

The only conversion trafex hands to iconv is UTF-32LE: lxml's UCS-4 buffer for a str with a leading U+FEFF and
astral characters (`dom/parse.rs` `pep393_buffer`). Bytes input always uses libxml2's built-in UTF-8 handler.
Rust strings are valid Unicode, and every implementation decodes valid UTF-32LE identically; the `bom_astral`
fixture in `tests/` checks this on every wheel target. On MSVC CMake names the static library `libxml2s.lib`.

## Source patches

Behavior stays byte-identical to upstream (lxml 6.1.3 bundles the same release). Every change is marked
`/* trafex: see crates/libxml2-sys/PATCHES.md */` in the source.

### Dict-owned names skip the re-lookup (tree.c)

`xmlStaticCopyNode` and `xmlNewDocProp` intern each element or attribute name with `xmlDictLookup(doc->dict, name)`.
trafex puts every document on one shared dict, so during copies the name is usually already that dict's entry,
and the lookup hashes it only to return the same pointer. When `xmlDictOwns(doc->dict, name)` holds, the
pointer is reused. The result is the same because dict strings are unique and names always point at the start of
an entry.
