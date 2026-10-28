# Changes to `goldens.json`

Every regeneration is recorded here with its reason (`docs/composition-views-scene.md`: a
failing parity test is never fixed by regenerating; a change in what a renderer draws is).

## feat/composition-views-quality (on 1a13459, PR #7)

Hash and counts: `sha256` of `goldens.json` is in the PR body. `manifests`: 100, of which 95 are
byte-identical to PR #7's; `transition_manifests`: 10, new.

| manifest(s) | change | why |
|---|---|---|
| `operad` / `connect4` / `default`, `payment2-pushout`, `no-links`, `release-undo`, `plan-threats` (5) | `+ hole root {accepts: "Root"}` | pflow-rs's plan `root` field (aacb7eb) is a slot of the solver template (spec D31, D19). No `piece root/…` entry: the default plan names no root piece. |
| `string` / `orders` / the 5 states, `string` / `connect4` / the 5 states (`transition_manifests`, new) | new array, 10 manifests with `level: "transition"` | the string view's transition level now draws diagram-viewer.js's layout of the flat net / Connect-4 and collects its elements (spec D27-D30). |

The other 95 manifests are unchanged although every view was redrawn: the richer drawings add
no element the spec lists. The token game, scenario, step and rewrite selections do not enter a
manifest (D32).
