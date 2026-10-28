# Compare our labels with Pons: python3 pons_compare.py - pons-scores.txt meta.jsonl
# (meta.jsonl from pons_input.py; the first argument is unused).
import json, sys, collections
S = sys.argv[1]
pons = {}
for l in open(sys.argv[2]):
    s, *v = l.split(); pons[s] = list(map(int, v))
sg = lambda x: (x > 0) - (x < 0)
c = collections.Counter(); bad = []
for l in open(sys.argv[3]):
    m = json.loads(l); sc = pons[m["seq"]]
    truth = [sg(sc[col]) for col in m["cols"]]
    assert all(sc[col] != -1000 for col in m["cols"]), m
    if m["vals"] is None:
        c["skipped"] += 1; c[f"skipped, true value {max(truth):+d}"] += 1; continue
    c["labelled"] += 1
    if m["vals"] == truth: c["every move agrees"] += 1
    else: bad.append((m["seq"], m["vals"], truth))
    for a, b in zip(m["vals"], truth): c[f"move ours {a:+.0f} pons {b:+d}"] += 1
    best = lambda v: {i for i, x in enumerate(v) if x == max(v)}
    if best(m["vals"]) == best(truth): c["same best-move set"] += 1
for k in sorted(c): print(f"{c[k]:6d}  {k}")
for b in bad[:5]: print("disagree:", *b)
