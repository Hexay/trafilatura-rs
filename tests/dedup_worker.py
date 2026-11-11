"""Runs a deduplicate=True config over the fixtures in a fresh process, since both engines keep a process-global LRU.
usage: dedup_worker.py <trafilatura_rs|trafilatura> <cfg> <passes>; prints the outputs as a JSON list."""
import json
import sys

from parity import CONFIGS, pages, run

engine, cfg, passes = sys.argv[1], sys.argv[2], int(sys.argv[3])
docs = pages()
print(json.dumps([run(engine, d["html"], CONFIGS[cfg]) for _ in range(passes) for d in docs]))
