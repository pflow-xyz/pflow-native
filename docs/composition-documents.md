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
| Receipts for runs, checks and content | `bundle.receipt`, `content.receipt`, `provenance.verify_receipt` (extended) | `composition/receipt.rs`, `commands/composition_receipt.rs`, `commands/provenance.rs` (the dispatch) |

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
or structured value therefore travels as a string whose bytes the hash covers. Canonical means
`model_id::canonical_json`'s rule: object keys sorted bytewise at every depth, no whitespace, numbers
spelled as `JSON.stringify` does, strings escaped as it does (no `<>&` escaping). **A `params` string is
sealed as received and never re-canonicalised** (the identity rule): it is parsed to build the subnet, and
`bundle.export` writes the same text back, so `cid` is the seal of the document received even for a
respaced or `\u`-escaped string. The loader also accepts `params` as an object (the prototype's shape) and
writes the canonical text of it, so an object and its canonical string are one document. `instances` and
`links` are sets to the sealer and are not ordered by it; their order in the text is preserved by the
store, not by the CID. A top-level `@id` is accepted when it is the document's own CID (what `store.get`
returns) and is not written by an export.

Errors (all `[CODE] message`): `COMPOSITION_TYPE`, `INSTANCE_DUPLICATE`, `INSTANCE_ID`,
`PATTERN_UNKNOWN`, `PARAMS_INVALID`, `INSTANCE_BUILD` (the constructor refused the parameters; its
message follows), `LINK_REFUSED` (pflow-compose's verdict and the page rules, with their codes).

### 1.2 Form (b): a raw `PetriNetBundle` (`@type: "PetriNetBundle"`)

Any Bundle in go-pflow's `metamodel.Bundle` shape (for example pflow-xyz
`examples/showcase/cafe.bundle.json`). It is parsed by `pflow_compose::Bundle`, which drops unknown
keys, so the **loaded JSON is kept beside the parsed bundle**: `bundle.export` returns it unchanged
while the composition is unedited (the CID then equals the sealed bytes as received), and re-serialises
the struct once it is edited.

A part named by `cid` is resolved **only to flatten and draw**: the received subnet (its `cid`, no `model`)
stays the document, so `cid` is the seal of what was received, the model CID is the one the identity rule
gives (a bare wrapper of one by-reference part *is* that net, `reduced: true`), and an unedited export is the
bytes received. An edit that does not touch a part or link writes it back as received, with its `cid` and
every key beside it; a changed or new one is what pflow-compose serialises plus the unknown keys its received
form had. So `preserved_keys` (and the `KEYS_PRESERVED` text) names exactly the keys the next export carries.

What loads and what is refused:

| Input | Result |
|---|---|
| a Bundle with inline `model` subnets | loads; validated; its report is returned, errors do not block the load |
| a subnet with `cid` and no `model` | resolved from the local store (a stored net's compiled model draws it) when present, else `[BUNDLE_PART_UNRESOLVED]` naming the CID; the document keeps the reference |
| `@context` missing or not `https://pflow.xyz/schema` | `[DOCUMENT_CONTEXT]` |
| the `tokenmodel/subnet` shape (`from_subnet` links) | `[BUNDLE_SHAPE_TOKENMODEL]` |
| `@type` neither of the two | `[DOCUMENT_TYPE]` (a document with no `@type` and a `subnets` array is read as a Bundle) |
| links whose ends are `place` or `transition` rather than `port` (cafe's) | load; an end is written `subnet:port`, `subnet:@place/<id>` or `subnet:@transition/<id>` in link keys |

A raw bundle has no pattern, so each subnet gets an `InstanceSpec` with `pattern: ""` (specs stay 1:1
with subnets; every consumer already treats an unknown pattern as "no ontology"). Optional `pattern`
and `params` keys on a subnet (the identity runs' recipe) are preserved by the unedited round trip and
are not interpreted until pflow-compose has the fields.

### 1.3 Identity, stated plainly

`bundle.load`, `bundle.export` and `store.put` report `cid`: the pflow seal of the exported document
(`cid::seal`, as written). `bundle.load` and `bundle.export` also report what `model_id` (the identity
rule, `crates/core/src/model_id.rs`) says about that document: `model_cid` (the part's CID when the
document is a bare wrapper), `reduced` and `parts` (the by-reference CIDs), or `model_error` with the
`[E_…]` text when the rule refuses the shape. That check is a report, not a gate: `bundle.load` accepts
shapes the rule refuses. For form (a) they also report `bundle_cid`, the seal of the plain Bundle that
form flattens to (what a Bundle-reading tool such as pflow.xyz or go-pflow would hash). A
PatternComposition has no part CIDs, so it is its own `@type` with CID = seal(doc); `tests/composition_documents.rs`
(`the_exported_documents_agree_with_the_identity_rule`, `the_by_reference_identity_vectors_load_to_their_golden_cids_and_export_unchanged`)
pins that `model_id(doc).sealed` equals the CID the commands report, for both forms, the cafe Bundle and the
four by-reference vectors of `tests/fixtures/model/`. A ModelContent CID is a handle for storing and
receipting that document, not the identity of a model. This feature changes no net's CID.

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
| `CONTENT_LIMIT` | error | more than 1000 entries, or an entry with more than 32 KiB of text or more than 512 `about` ids (a check's 16-step cap is `CHECK_INVALID`; a document over 4 MiB is refused `[DOCUMENT_TOO_LARGE]` before it is read) |

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

`content.library` lists them. Library content is attached to a catalog-opened composition, and to a loaded
PatternComposition whose exported CID is the one the content was written against (the example, reloaded);
any other loaded document (an edited example, a raw Bundle) is bare unless the call passes `content`.
The statics `comp::scenarios()` and `comp::scenario(id)` are now derived from `orders` content (parsed
once), so the four orders scenarios have one source.

### 3.6 Where the app shows it

`crates/app/src/modelcontent.rs` draws a collapsible "Model content" bar under the composition views, for the
view and example on screen, from `content.get {view}` (`{subject: "plan", view}` for `connect4`) and
`bundle.state`, read once per `(revision, view, example)` and drawn from the cached result. Top to bottom: the
view's `convention-example` text; "What this model lacks" (the `missing` list, the bare sentence when no content
is attached, and the count of dormant entries); then the entries by kind (`finding`, `caveat`, `judgment`,
`note`), each a collapsing header with its text, check claim and cites. A `scenario` entry is not listed (the
Process view and the lens test draw it). "Run checks" hands back one `content.check {entry}` per frame (with
`subject: "plan"` for `connect4`); the app queues each and the mark is read from the report that command
stored, so the panel never edits the session. The panel is outside the parity manifest: the goldens do not
change.

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
| `content.receipt` | no | signs a `composition-check` receipt for `entry`, or a `composition-content` receipt over the strict validation and every check (§6); a failing check is receipted as it is |

### `bundle.receipt`

`scenario` (a declared scenario id) or `script` (`["orders/dequeue", …]`): runs it on the flat net and
signs `{model: <composition document cid>, scenario: {kind: "composition-run", …}, resultHash}` with the
receipt key (`provenance::issue`, sim's Ed25519 canonical-JSON scheme). Both commands are queries (not
journaled), return `{result, receipt, report}` as `sim.receipt` does and store the report.

### Changes to existing commands

* `bundle.state` gains `document`, `capabilities`, `missing`, `content` (a summary). Link ends gain
  `place`/`transition` when a link names those instead of a port. Nothing is removed.
* `bundle.scenarios` returns the open composition's declared scenarios, else (no composition open) the
  `orders` library's; the element shape is unchanged. `bundle.run {scenario}` resolves a declared
  scenario of the open composition.
* `store.put` takes `composition: true` (the open composition's document; `form` as in `bundle.export`),
  `content: true` (its content, stored with `about` = the composition's CID), or `document` (any
  document `store::kind_of` knows). `store.open` opens by kind; `store.list` shows `kind` and `about`.
* `provenance.verify_receipt` replays `composition-run`, `composition-check` and `composition-content` receipts (§6).

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

`provenance::issue` signs; its `Receipt` is unchanged (`deny_unknown_fields` and all). Three new
`scenario.kind` values. For all of them `model` is the composition document's CID
(`Composition::document_cid`, the seal of what `bundle.export` writes), `modelRates` is absent and
`engine` is `pflow-compose`:

| `scenario.kind` | `scenario` holds | `resultHash` is over |
|---|---|---|
| `composition-run` | `{kind, form, content: <cid|null>, scenario: {id, title, script, expect}}`; `content` is the content document the scenario was declared in, null for a bare `script` | the run result (`comp::Run` as JSON: steps, `completed`, final marking, expect rows), canonical |
| `composition-check` | `{kind, form, content: <cid>, entry, check}`; `check` is the entry's check, in full | the check result (§3.4: `{id, ok, claim, steps}`), canonical |
| `composition-content` | `{kind, form, content: <cid>}` | `{valid, unresolved: [{entry, ref}], checks: [{id, ok}]}`: strict `content.validate` (`valid` is its verdict, `unresolved` the element ids that did not resolve) and the outcome of every check, each list in id order (the document's array order is not part of its CID, so it is not part of the result) |

"Canonical, hashed" is `checks::result_hash`: sha256 over the sorted-key JSON `provenance::canonical`
writes. A receipt states what happened and not what should: a run that stops or fails its predicates, a
check whose `ok` is false and a content with a failing check are receipted as they are, and replay
(not the outcome) is what `valid` means. The `form` is the composition document's form (`composition` or
`bundle`).

`bundle.receipt` and `content.receipt` issue them by running the replay below on the scenario as the
receipt will store it, so an issued receipt replays in the session that issued it.

`provenance.verify_receipt` dispatches on `scenario.kind` (any `composition-` kind; one this build does
not know is `uncheckable`). It checks the signature, finds the composition by `model`, replays and
compares the hash, with the same keys and the same `valid` / `invalid` / `uncheckable` verdicts it gives
a net receipt (`kind`, `modelFrom` and, for a check or content receipt, `contentFrom` are added):

* **The composition** is the open one when its document CID is `model`, else the stored document of
  that CID (read back with the form its `@type` says, and only trusted when it exports as exactly that
  CID). Neither: `uncheckable`, "not found". A composition edited after the receipt therefore verifies
  it if the original document is in the store (`store.put`), and is `uncheckable` if it is not.
* **The content** (check and content receipts) is the document of the CID in `scenario.content`: the
  one attached to the composition found or to the open one (`contentFrom` `library:<id>` when that is
  the library's, which a composition that is a library example gets back whenever it is loaded, else
  `current`), a library entry, or a stored document (`store`). The
  check or the content runs against it (its declared scenarios are what `bundle.run {scenario}` reads),
  not against whatever content is attached now. None found: `uncheckable`.
* **The replay** is the run on the flat net, the check on a scratch session (§3.4, so only `bundle.*`
  and `plan.*` queries run, whatever the receipt says), or the strict validation plus every check.
  A bad signature, or a replay that does not reproduce `resultHash`, is `invalid`.

A replay reproduces the labels a check can read only as the document rebuilds them: a check that
asserts on `bundle.state`'s `document.origin` or `content.source` can differ between the session that
issued the receipt (a catalog-opened composition) and a session that rebuilds it from the store.
The `why` strings of a run are pflow-metamodel's, so a receipt is bound to the pflow-rs revision
(`revisionAtIssue`), as a net receipt is to its engine.

## 7. Tests

* `tests/composition_documents.rs`: both forms round trip (`orders`: build → export (a) → load → identical
  `bundle.state` links, identical `bundle.flatten`, identical re-export and CID; cafe: load → export is the
  loaded JSON and seals to `z4EBG9j1SUhuY7XnEcPfHQPzQkmqjUtgrD2wHADw8qdRbpnNt6W`; edited → re-serialised),
  reordered instances and links keep the CID, a reordered saga `steps` does not, every refusal code.
* `tests/model_content.rs`: the validator for each code; the library content validates against its
  composition; **every check in every library entry is run and passes**; the checks can fail (the pool-leak
  finding's check goes red if the pool's release link is fused with `undo_reserve` instead, and so do three
  more orders findings, a declared scenario's prediction and the cafe guard finding when their claim is
  made false); the `content.*` commands (filters, dormant entries, one undo step, a check leaves the
  session alone); the element ids of §3.2, each for the right thing and not the wrong one; the cafe
  Bundle's content (declared scenarios, a check on a raw Bundle); the declared scenarios
  (`bundle.scenarios`, `bundle.run`, the lens test).
* `tests/composition_store.rs`: put, get, list, open and delete of each kind; `dump_blobs` →
  `replace_all` round trip.
* `tests/composition_receipts.rs`: a run, a check and a content receipt sign and verify (on the open
  composition, on one read back from the store after `store.put`, and on the library's content); a
  tampered signature, script, check, hash or model CID is `invalid` or `uncheckable`; a failing check
  and a run that stops are receipted as they are; an edited composition verifies the old receipt when the
  original is stored and is `uncheckable` when it is not; the content is pinned by its CID; net receipts
  verify as before.
* `crates/app/tests/model_content_panel.rs`: the panel drawn headless: the convention example first, the
  lacks section above the groups, a raw Bundle that says it is bare, both examples, the cache key, a check that
  goes red marked as failed, the plan's checks run with `subject: "plan"`, and no trace left in the session.
* The existing `views_*` and `compose` tests pass unchanged.
