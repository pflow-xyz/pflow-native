# Composition documents, model content, storage and receipts

Status: implemented in `feat/composition-documents-store` (one PR). Design context: pflow-xyz PR #124,
`docs/design/models-first.md` §1.4 (model format), §1.5 and D11 (curated content), D12 (store and
receipts). This file is the contract for `bundle.load`, `bundle.export`, the `content.*` commands,
`store.put` for documents, and composition receipts. It is also the spec the tests are written against.

## 0. What this adds, in one table

| Need | Command(s) | Module |
|---|---|---|
| Open a composition from a document (two forms) | `bundle.load` | `composition/document.rs`, `commands/bundle_doc.rs` |
| Write the open composition out again | `bundle.export` | same |
| Curated content per model (D11) | `content.library`, `content.get`, `content.load`, `content.export`, `content.clear`, `content.validate`, `content.check` | `composition/content.rs`, `composition/checks.rs`, `commands/content.rs` |
| Store a composition or content document (D12) | `store.put` (extended), `store.open`, `store.list` | `store/kind.rs`, `store/mod.rs`, `commands/store.rs` |
| Receipts for runs, checks and content | `bundle.receipt`, `content.receipt`, `provenance.verify_receipt` (extended) | `composition/receipt.rs`, `commands/content.rs` |

Nothing here changes `crates/core/src/cid.rs`: every CID in this feature is `cid::seal(&Value)` on the
JSON the command exported (the identity runs' contract: sealed bytes are the document as written).

## 1. Two document forms

`bundle.load {document}` takes either; the form is read from `@type`.

### 1.1 Form (a): a pattern-instance composition (`@type: "PatternComposition"`)

The prototype's `INSTANCE_SPECS` shape (`{id, pattern, params}`) plus typed links. The subnets are
built by pflow-compose's constructors (`new_queue`, `new_resource_pool`, `Saga::builder`), so nothing
about the net is stored: only the recipe.

```json
{
  "@context": "https://pflow.xyz/schema",
  "@type": "PatternComposition",
  "name": "orders",
  "description": "queue ⊗ resource pool ⊗ saga, joined by the three event idioms",
  "instances": [
    {"id": "orders",  "pattern": "queue",         "label": "orders",
     "why": "Bounded (capacity 3) so the capacity invariant exists; …",
     "params": "{\"Capacity\":3,\"ID\":\"orders\",\"Initial\":2,\"Payload\":\"\"}"},
    {"id": "workers", "pattern": "resource-pool", "params": "{\"capacity\":2,\"id\":\"workers\",\"name\":\"workers\"}"},
    {"id": "payment", "pattern": "saga",          "params": "{\"accounts\":{…},\"id\":\"payment\",\"steps\":[…]}"}
  ],
  "links": [
    {"kind": "event", "from": {"subnet": "orders", "port": "dequeue"}, "to": {"subnet": "workers", "port": "acquire"}},
    {"kind": "event", "from": {"subnet": "orders", "port": "dequeue"}, "to": {"subnet": "payment", "port": "do:reserve"}},
    {"kind": "event", "from": {"subnet": "workers", "port": "release"}, "to": {"subnet": "payment", "port": "commit"}}
  ]
}
```

Fields:

| Field | Required | Meaning |
|---|---|---|
| `@context` | yes | `https://pflow.xyz/schema` (the preloaded context; nothing else is accepted) |
| `@type` | yes | `PatternComposition` |
| `name`, `description` | no | text |
| `namespace`, `arc_merge` | no | passed to the Bundle (`namespace` bool, `arc_merge` `sum` or `max`) |
| `instances[]` | yes | `id` (unique, no `:` or `/`), `pattern` (`queue`, `resource-pool` or `saga`), `params`, optional `label` (default `id`) and `why` (the author's reason for the parameters; judgment) |
| `links[]` | no | `kind` (`token`, `data`, `event`, `guard`), `from` and `to` (`{subnet, port}`), optional `id`, `rename`, `condition`, `lowering` (pflow-compose's Link fields) |

**`params` is a canonical-JSON string in the exported form.** Only `arcs`, `token`, `weight`,
`capacity`, `initial` and `parents` are `@list`s to the sealer (`cid.rs` `LIST_TERMS`); every other
array hashes as an unordered set, so a saga's `steps` reordered would seal to the same CID. An ordered
or structured value therefore travels as a string whose bytes the hash covers. Canonical means: object
keys sorted bytewise at every depth, no whitespace, integers as integers, strings escaped as
`JSON.stringify` does (no `<>&` escaping). The loader also accepts `params` as an object (the
prototype's shape) and canonicalises it, so `load → export` is stable. `instances` and `links` are sets
to the sealer and are not ordered by it; their order in the text is preserved by the store, not by the CID.

Errors (all `[CODE] message`): `COMPOSITION_TYPE`, `INSTANCE_DUPLICATE`, `INSTANCE_ID`,
`PATTERN_UNKNOWN`, `PARAMS_INVALID`, `INSTANCE_BUILD` (the constructor refused the parameters; its
message follows), `LINK_REFUSED` (pflow-compose's verdict and the page rules, with their codes).

### 1.2 Form (b): a raw `PetriNetBundle` (`@type: "PetriNetBundle"`)

Any Bundle in go-pflow's `metamodel.Bundle` shape (for example pflow-xyz
`examples/showcase/cafe.bundle.json`). It is parsed by `pflow_compose::Bundle`, which drops unknown
keys, so the **loaded JSON is kept beside the parsed bundle**: `bundle.export` returns it unchanged
while the composition is unedited (the CID then equals the sealed bytes as received), and re-serialises
the struct once it is edited.

What loads and what is refused:

| Input | Result |
|---|---|
| a Bundle with inline `model` subnets | loads; validated; its report is returned, errors do not block the load |
| a subnet with `cid` and no `model` | resolved from the local store (a stored net becomes the inline subnet) when present, else `[BUNDLE_PART_UNRESOLVED]` naming the CID |
| the `tokenmodel/subnet` shape (`from_subnet` links) | `[BUNDLE_SHAPE_TOKENMODEL]` |
| `@type` neither of the two | `[DOCUMENT_TYPE]` |
| links whose ends are `place` or `transition` rather than `port` (cafe's) | load; an end is written `subnet:port`, `subnet:@place/<id>` or `subnet:@transition/<id>` in link keys |

A raw bundle has no pattern, so each subnet gets an `InstanceSpec` with `pattern: ""` (specs stay 1:1
with subnets; every consumer already treats an unknown pattern as "no ontology"). Optional `pattern`
and `params` keys on a subnet (the identity runs' recipe) are preserved by the unedited round trip and
are not interpreted until pflow-compose has the fields.

### 1.3 Identity, stated plainly

`bundle.export` and `store.put` report `cid`: the pflow seal of the exported document. For form (a)
they also report `bundle_cid`, the seal of the plain Bundle that form flattens to (what a Bundle-reading
tool such as pflow.xyz or go-pflow would hash). Until models-first D1 is decided, **a PatternComposition
or ModelContent CID is a handle for storing and receipting that document, not the identity of a model**;
the identity runs mint no composition CIDs yet, and this PR changes no net's CID.

## 2. What each form can and cannot draw

The views read `bundle.state`, which now carries `document` (form, origin, name, cid), `capabilities`
and `missing` (a list of `{code, text}` written for a person). A view that cannot draw something shows
the `missing` text instead.

| Capability | PatternComposition | Raw Bundle |
|---|---|---|
| Flatten, flat net, wires, fused groups, incidence, token game | yes | yes |
| `bundle.validate` report | yes | yes |
| Port schema colours, `pattern_port` ontology keys | yes | only if the subnets declare ports; no ontology keys (`NO_PATTERN`) |
| Snap-together candidates, `bundle.add_link` verdicts | yes | for the ports it declares; cafe declares none (`NO_PORTS`), so only existing links are drawn |
| Idiom and cq/13 lighting, pool-leak flag (`idiom_of`) | yes (by pattern) | no (`NO_PATTERN`) |
| Operad holes, pattern hovers | yes | no |
| Scenario runs, Process view | the scenarios its content declares | same; none declared → `NO_SCENARIOS` ("no scenario declared") |
| Lens test | strict test always; fixes and the commit/refund reading when `commit` and `refund` are declared | strict test when scenarios are declared, else `NO_SCENARIOS` |
| Findings, notes, convention examples | its content, else `NO_CONTENT` | same |
| Layout | computed by the views (neither form stores positions: `NO_LAYOUT`) | same |

`missing` codes: `NO_PATTERN`, `NO_PORTS`, `NO_SCENARIOS`, `NO_CONTENT`, `NO_LAYOUT`, `FLATTEN_FAILED`,
`KEYS_PRESERVED` (the document has keys this build keeps but does not interpret).

## 3. Model content (D11)

A content document says what a model means, in data keyed by the model's own element ids, so a view
reads it instead of hard-coding it per example. Library entries ship content; a user's composition is
bare and says so (`NO_CONTENT`).

```json
{
  "@context": "https://pflow.xyz/schema",
  "@type": "ModelContent",
  "name": "orders",
  "subject": {"kind": "composition", "name": "orders"},
  "composition": "z…",
  "entries": [
    {"id": "pool-leaks-on-rollback", "kind": "finding", "order": 10,
     "title": "The pool leaks a worker on rollback, and conservation still holds",
     "text": "In the refund scenario the worker stays in workers/in_use …",
     "about": ["place:workers/in_use", "link:event:workers:release->payment:commit", "scenario:refund",
               "constraint:workers/pool_conservation"],
     "views": ["flat", "process"],
     "cites": [{"repo": "ontology-pflow-xyz", "rev": "bc4a996", "path": "ns/patterns.ttl", "from": 496, "to": 496,
                "note": "pat:idiom-pool-release-saga-commit caveat"}, {"bib": "baez-master"}],
     "check": "{\"claim\":\"…\",\"steps\":[…]}"}
  ]
}
```

### 3.1 Entry fields

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | unique in the document; `[a-z0-9][a-z0-9._-]*` |
| `kind` | yes | `finding` (a claim a check can back), `judgment` (the author's choice, labelled as such), `caveat` (a limit the reader must know), `scenario` (a declared firing script), `convention-example` (the text a view shows for this model in its convention panel), `note` (a hover note on an element) |
| `text` | yes (except `scenario`, where it is optional) | prose |
| `title` | no | one line |
| `order` | no | integer; sorts entries (the hash does not preserve array order, so authored order is a scalar) |
| `about` | yes, ≥ 1 | element ids (§3.2) the entry is attached to; model-wide is `["model:"]` |
| `views` | no | view ids that show it (`cospan spiders operad string flat boundaries lens incidence process laws`); `convention-example` needs exactly one |
| `cites` | no | `{repo, rev, path, from, to, note}` (a source cite) or `{bib: "<key>"}` (a key of the ontology's `bib`) |
| `check` | no | canonical-JSON string of a check (§3.4) |
| `scenario` | `scenario` kind only | canonical-JSON string of `{script: ["<subnet>/<transition>", …], expect: [{place, op, n, says}], note?}`; `op` is `==` `!=` `<` `<=` `>` `>=` |

### 3.2 Element ids (`about`)

An id is `<type>:<ref>`, split at the first `:`. Explicit types keep `orders/dequeue` (a transition)
apart from `orders:dequeue` (a port).

| Type | `<ref>` | Resolves when |
|---|---|---|
| `model` | empty | always |
| `instance` | `<subnet>` | a subnet of that id exists |
| `place` | `<subnet>/<place>` | the subnet's net has that place |
| `transition` | `<subnet>/<transition>` | the subnet's net has that transition |
| `port` | `<subnet>:<port>` | the subnet declares that port |
| `link` | `<kind>:<subnet>:<port>-><subnet>:<port>` (`comp::link_key`; ends without a port as in §1.2; an event link matches either way round) | a link with that key is in the composition |
| `constraint` | `<id>` (bundle level) or `<subnet>/<id>` | declared there |
| `scenario` | `<id>` | the document declares a `scenario` entry of that id |
| `flat-place`, `flat-transition` | the flat net's id (`wire:…`, `fused:…`, `orders/items`) | the flat net has it (needs `bundle.flatten` to succeed) |
| `idiom` | an ontology idiom key (`idiom-pool-release-saga-commit`) | the ontology has it |
| `view` | a view id | it is one of the ten |

For a `plan` subject (the Connect-4 search plan): `piece:<name>`, `slot:<name>`, `net-place:<id>`,
`net-transition:<id>`, `view:<id>`, `model:`.

### 3.3 Validation

`content.validate` (and every load) reports `{valid, errors[], warnings[]}`, each `{code, entry?, ref?,
message}`:

| Code | Level | Meaning |
|---|---|---|
| `CONTENT_TYPE`, `CONTENT_FIELD` | error | wrong `@type`, a missing or mistyped field |
| `ENTRY_DUPLICATE`, `ENTRY_ID`, `ENTRY_KIND`, `ENTRY_ABOUT_EMPTY` | error | structure |
| `ENTRY_VIEW` | error | an unknown view id; a `convention-example` without exactly one view |
| `ID_UNRESOLVED` | error under `strict` (the default for an explicit load), warning otherwise (the entry is dormant: views skip it) | an `about` id that does not resolve against the composition |
| `ID_SYNTAX` | error | not `<type>:<ref>`, or an unknown type |
| `SCENARIO_INVALID` | error | the `scenario` JSON does not parse, a member or place is not in the net, an unknown `op` |
| `CITE_BIB_UNKNOWN` | error | a `bib` key the ontology does not have |
| `CHECK_INVALID`, `CHECK_COMMAND_FORBIDDEN` | error | §3.4 |
| `COMPOSITION_CID_STALE` | warning | `composition` names a CID other than the open composition's |

### 3.4 Checks

A finding that the registry can decide carries the check that decides it, and a test runs it.

```json
{"claim": "the refund run completes with the worker still in use, and conservation holds",
 "steps": [
   {"command": "bundle.run", "args": {"scenario": "refund"},
    "expect": [{"path": "/completed", "op": "==", "value": true},
               {"path": "/expect/2/pass", "op": "==", "value": false}]},
   {"command": "bundle.incidence", "args": {},
    "expect": [{"path": "/invariants/0/holds", "op": "==", "value": true}]}]}
```

* `command` must be a registry command in the `bundle.` or `plan.` namespace that is a query
  (`journal: false`); `content.*` and every editing command are refused (`CHECK_COMMAND_FORBIDDEN`), so a
  document from outside cannot run anything but a read of the model.
* `expect[].path` is an RFC 6901 JSON Pointer into the command's result; `op` is `==`, `!=`, `<`, `<=`,
  `>`, `>=`, `contains` (string or array), `exists` or `absent`.
* Checks run on a scratch `Session` that holds a clone of the composition (and plan), so they leave the
  caller's session, artifacts and undo history alone and replay the same anywhere.
* The result is `{id, ok, claim, steps: [{command, args, ok, error?, expect: [{path, op, value, actual,
  ok}]}]}`; its `sha256:` over the canonical JSON is what a check receipt signs.

### 3.5 Library content

| Library id | Subject | Attached when |
|---|---|---|
| `orders` | the orders/workers/payment composition | `Composition::open` is given a set of instances that includes `orders`, `workers` and `payment` (so `bundle.open` with defaults) |
| `connect4` | the Connect-4 search plan | `plan.open` (read through `content.get {subject: "plan"}`) |

`content.library` lists them. Library content is attached to a catalog-opened composition only; a
loaded document is bare unless the call passes `content`. The statics `comp::scenarios()` and
`comp::scenario(id)` are now derived from `orders` content (parsed once), so the four orders scenarios
have one source.

## 4. Commands

Arguments and results are JSON. `Json` arguments accept the value or a string holding it.

### `bundle.load` (edit, journaled, one undo step)

`document` (Json, a form (a) or (b) document) **or** `cid` (a stored document), optional `content`
(Json, a ModelContent document; or `library: "<id>"`), optional `strict` (default true).
Result: `{form: "composition"|"bundle", name, cid, bundle_cid?, instances, links, validation,
flatten: {ok, error?}, missing, content: {source, entries, unresolved}}`. Replaces any open composition.

### `bundle.export` (query, stores a Report artifact)

`form` (`auto` default: the form it was loaded or built as; `composition`, `bundle`), `content` (bool).
Result: `{form, document, cid, bundle_cid?, content?: {document, cid}}`. `composition` from a
composition with any `pattern: ""` instance is refused `[EXPORT_NEEDS_PATTERNS]`. `bundle` from a
raw-loaded, unedited composition returns the loaded JSON; otherwise the Bundle pflow-compose serialises
(with `@context`/`@type`). The built-in orders example exports as form (a) and reloads identically.

### `content.*`

| Command | Journal | Does |
|---|---|---|
| `content.library` | no | the library entries: `{id, subject, title, entries, checks}` |
| `content.get` | no | `{source, subject, entries, unresolved}` for the open composition (default) or `subject: "plan"`; filters `kind`, `about` (an element id), `view`; resolved entries only unless `all: true` |
| `content.load` | yes | `document` (Json) or `library`; `strict` (default true); attaches to the open composition |
| `content.export` | no | `{document, cid}` (stores a Report) |
| `content.clear` | yes | back to bare |
| `content.validate` | no | the §3.3 report (stores a Report) |
| `content.check` | no | runs one entry's `check` (`id`) or all (`all: true`) on a scratch session; `{checks, passed, failed}` (stores a Report) |
| `content.receipt` | no | signs a check receipt for `entry`, or a content receipt (all ids resolve and all checks pass) |

### `bundle.receipt`

`scenario` (a declared scenario id) or `script` (`["orders/dequeue", …]`): runs it on the flat net and
signs `{model: <composition document cid>, scenario: {kind: "composition-run", …}, resultHash}` with the
receipt key (`provenance::issue`, sim's Ed25519 canonical-JSON scheme).

### Changes to existing commands

* `bundle.state` gains `document`, `capabilities`, `missing`, `content` (a summary). Link ends gain
  `place`/`transition` when a link names those instead of a port. Nothing is removed.
* `bundle.scenarios` returns the open composition's declared scenarios, else (no composition open) the
  `orders` library's; the element shape is unchanged. `bundle.run {scenario}` resolves a declared
  scenario of the open composition.
* `store.put` takes `composition: true` (the open composition's document; `form` as in `bundle.export`),
  `content: true` (its content, stored with `about` = the composition's CID), or `document` (any
  document `store::kind_of` knows). `store.open` opens by kind; `store.list` shows `kind` and `about`.
* `provenance.verify_receipt` replays `composition-run` and `composition-check` receipts.

## 5. The store (D12)

`store::kind_of(&Value) -> DocKind` classifies by `@type`:

| `DocKind` | `@type` | In the index |
|---|---|---|
| `Net` | `PetriNet`, or any other `@type` that is not one of these with `places` and `transitions` (a `ResourceNet`; the legacy untyped net) | `kind` omitted, so a net's index entry is byte-for-byte what it was |
| `Bundle` | `PetriNetBundle` | `kind: "bundle"` |
| `Composition` | `PatternComposition` | `kind: "composition"` |
| `Content` | `ModelContent` | `kind: "content"`, `about: <composition cid>` |
| `Unknown` | anything else | refused |

`store.put` stores the open net (or `model`), the open composition (`composition: true`; `form` as in
`bundle.export`), its content (`content: true`, recorded `about` the composition's CID; with `composition`
both), or any `document` of a kind above. `store.open` opens by kind: a net as the open net; a composition
or bundle as `bundle.load {cid}` does (one undo step, and the library's content comes back when the CID is
the library example's); content attaches strictly to the open composition (`[NO_COMPOSITION]` when none is
open). `store.list {kind}` filters. Documents are opened from `Store::get_sealed`, the stored document
without the `@id` the store added, because the loaders refuse a key they do not know. `replace_all` refuses
a state whose index gives a model another kind than its document has, or holds a file of no kind the store
keeps.

`Store::put_model(&Document, …)` is now a thin wrapper over `Store::put_document(&Value, kind, …)`; the
file layout (`models/<cid>.jsonld`, `index.json`) and `dump_blobs` / `replace_all` are unchanged, so
stored compositions survive `session.dump_state` / `restore_state` and the browser's saved `store` part
with no new key. sim's predicate registry is untouched: a content document is related to its composition
by `about` in its index entry, because sim's `about` edge needs an Artifact or Map subject.

## 6. Receipts

`provenance::issue` signs; its `Receipt` is unchanged (`deny_unknown_fields` and all). Two new
`scenario.kind` values:

| `scenario.kind` | `model` | `scenario` holds | `resultHash` is over |
|---|---|---|---|
| `composition-run` | the composition document's CID | `{kind, form, scenario: {id, title, script, expect}}` | the run result (`comp::Run` as JSON, canonical) |
| `composition-check` | the composition document's CID | `{kind, form, content: <cid|null>, entry, check}` | the check result (§3.4), canonical |

`provenance.verify_receipt` checks the signature, finds the composition (the open one, a stored one by
CID), replays the script or the check on a scratch session and compares the hash, with the same
`valid` / `invalid` / `uncheckable` verdicts it gives a net receipt.

## 7. Tests

* `tests/composition_documents.rs`: both forms round trip (`orders`: build → export (a) → load → identical
  `bundle.state` links, identical `bundle.flatten`, identical re-export and CID; cafe: load → export is the
  loaded JSON and seals to `z4EBG9j1SUhuY7XnEcPfHQPzQkmqjUtgrD2wHADw8qdRbpnNt6W`; edited → re-serialised),
  reordered instances and links keep the CID, a reordered saga `steps` does not, every refusal code.
* `tests/model_content.rs`: the validator for each code; the library content validates against its
  composition; **every check in every library entry is run and passes**; the pool-leak finding's check
  goes red if the pool's release link is fused with `undo_reserve` instead.
* `tests/composition_store.rs`: put, get, list, open and delete of each kind; `dump_blobs` →
  `replace_all` round trip; receipts sign, verify, and fail on a tampered script, hash or composition.
* The existing `views_*` and `compose` tests pass unchanged.
