import json, sys
# Referee positions (dense markings, header place order) -> Pons move strings.
f = open(sys.argv[1]); h = json.loads(next(f)); P = {p: i for i, p in enumerate(h["places"])}
rows = [json.loads(l) for l in f if l.strip()]
def board(mk):
    cols = []
    for c in range(7):
        col = []
        for r in range(6):
            x, o = mk[P[f"x_{c}_{r}"]], mk[P[f"o_{c}_{r}"]]
            if x or o: col.append("x" if x else "o")
        cols.append(col)
    return cols
def seq(cols):
    # Undo top pieces, last mover first: X moved last iff it has more pieces.
    n = sum(len(c) for c in cols); nx = sum(c.count("x") for c in cols)
    if nx not in (n - nx, n - nx + 1): return None
    memo = set()
    def go(cols, n):
        if n == 0: return []
        key = tuple(map(tuple, cols))
        if key in memo: return None
        side = "x" if n % 2 == 1 else "o"   # X plays odd plies
        for c in range(7):
            if cols[c] and cols[c][-1] == side:
                rest = [list(k) for k in cols]; rest[c].pop()
                s = go(rest, n - 1)
                if s is not None: return s + [c + 1]
        memo.add(key); return None
    return go(cols, n)
out = open(sys.argv[2], "w"); meta = open(sys.argv[3], "w")
for r in rows:
    cols = board(r["mk"]); s = seq(cols)
    if s is None: print("no sequence", cols, file=sys.stderr); continue
    open_cols = [c for c in range(7) if len(cols[c]) < 6]
    out.write("".join(map(str, s)) + "\n")
    meta.write(json.dumps({"seq": "".join(map(str, s)), "cols": open_cols, "vals": r["vals"]}) + "\n")
print(len(rows), "positions", file=sys.stderr)
