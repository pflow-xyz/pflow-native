# Label the positions of N sampled Connect-4 games and time it.
#   python3 label_games.py <pflow-native-cli> <labels.jsonl> <games>   (RAYON_NUM_THREADS=1 for one thread)
# Run from the repo root. The labels file is a cache: pass a path that does not
# exist yet, or the run only times the positions it has not seen.
# 250 games (seed 13) gives the 1,942-position referee; 40 is the timing run.
import json, subprocess, sys, time
G = {"sides": {"x_": 1, "o_": -1}, "reward": {"win_x": 1, "win_o": -1}, "auto": ["x_win_", "o_win_"]}
p = subprocess.Popen([sys.argv[1], "control"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
def run(i, c, a):
    p.stdin.write(json.dumps({"id": i, "command": c, "args": a}) + "\n"); p.stdin.flush()
    return json.loads(p.stdout.readline())
run(1, "file.open", {"path": "examples/connect-4.json"})
t = time.time()
r = run(2, "rnn.imitate", {**G, "arch": "mlp", "positions": "sampled", "epochs": 1, "seed": 13, "games": int(sys.argv[3]),
                           "labels": sys.argv[2], **json.loads(sys.argv[4] if len(sys.argv) > 4 else "{}")})
dt = time.time() - t
if "result" not in r: print(r); sys.exit(1)
smp = r["result"]["training"]["sample"]
print(f"{dt:.1f}s  {smp['positions']/dt:.1f} positions/s  labelled {smp['labelled']} skipped {smp['skipped']}")
