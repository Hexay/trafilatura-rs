# Local changes to the vendored libxml2 2.14.6

Behavior stays byte-identical to upstream (lxml 6.1.3 bundles the same release). Every change is marked
`/* trafex: see crates/libxml2-sys/PATCHES.md */` in the source.

## Dict-owned names skip the re-lookup (tree.c)

`xmlStaticCopyNode` and `xmlNewDocProp` intern each element or attribute name with `xmlDictLookup(doc->dict, name)`.
trafex puts every document on one shared dict, so during copies the name is usually already that dict's entry,
and the lookup hashes it only to return the same pointer. When `xmlDictOwns(doc->dict, name)` holds, the
pointer is reused. The result is the same because dict strings are unique and names always point at the start of
an entry.
