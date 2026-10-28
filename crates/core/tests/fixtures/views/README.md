# Composition-view receipts

Copied unmodified from `pflow-views-prototype` (local repo, commit `91695ad`),
`data/receipts/`. They record what the reference implementations said, and
`tests/views_compose.rs`, `tests/views_plan.rs` hold pflow-native's commands to them.

| File | What | Made by | sha256 |
|---|---|---|---|
| `gopflow.json` | go-pflow's own verdicts on the prototype's six instances: subnets from its constructors, `Bundle.Validate` on 5,576 ordered port pairs × link kinds, 11 `FlattenWithMap` configs, 4 scenario runs (`Model.Enabled` / `Fire`) | go-pflow `c802f34` via the prototype's `tools/gocheck` | `861c63b4ae9d841bfca04ec96653b3fa71e64a24e81d657c661d7f8dc1e1d402` |
| `plancheck.json` | pflow-rs `SearchPlan::from_json` / `check()` / `solver()` on 289 plans (Connect-4 and a goalless net), with solver prints on probe positions | pflow-rs `c396d46` via the prototype's `tools/plancheck` | `f61894dbb7b3accddc3dca5d725004c9cb310e4e0f64bf212703d89ef96f7405` |

Where pflow-native's pinned pflow-rs differs from them, the tests name each
difference and assert its count (see each test's module doc): pflow-rs 7ba0824's deliberate plan changes (built filters,
the longer "known" list, optional `filters`), plus three malformed-plan wordings
that serde_json's `preserve_order` (on in this workspace) changes.
