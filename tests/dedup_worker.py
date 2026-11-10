"""Runs the fineweb config (deduplicate=True) over the fixtures in a fresh process, since both engines keep a
process-global LRU. usage: dedup_worker.py <trafex|trafilatura> <passes>; prints the outputs as a JSON list."""
import json
import sys

from parity import CONFIGS, pages, run

engine, passes = sys.argv[1], int(sys.argv[2])
docs = pages()
print(json.dumps([run(engine, d["html"], CONFIGS["fineweb"]) for _ in range(passes) for d in docs]))
