#!/usr/bin/env bash
# Regenerate the trafilatura 2.3.1 metadata tables from .venv-t23: XPaths, the regexes changed since 1.11.0 (the
# others are shared with crates/trafilatura-rs/src/trafilatura/meta_regexes.rs) and the str.isalpha/punctuation tables.
set -euo pipefail
. "$(dirname "$0")/../tools/env.sh"
SRC=$ROOT/crates/trafilatura-rs/src
"$PY23" "$ROOT/harness/gen_meta_xpaths.py" > "$SRC/trafilatura2/metadata_xpaths.rs"
"$PY23" "$ROOT/harness/gen_unicode_tables.py" meta > "$SRC/py/tables_meta.rs"
"$PY23" "$ROOT/harness/gen_py_regexes.py" \
  "trafilatura 2.3.1 metadata.py and json_metadata.py patterns changed since 1.11.0 (harness/gen_meta23.sh)" \
  JSON_MINIFY=trafilatura.metadata:JSON_MINIFY \
  AUTHOR_EMAIL=trafilatura.json_metadata:AUTHOR_EMAIL \
  AUTHOR_SPLIT=trafilatura.json_metadata:AUTHOR_SPLIT \
  'CATEGORY_HREF=@:/categor(?:y|ies|s)?/' \
  'TAG_HREF=@:/tag(?:y|ies|s)?/' \
  > "$SRC/trafilatura2/meta_regexes.rs"
