# Composition documents, model content, storage and receipts

Status: implemented in `feat/composition-documents` (one PR). Design context: pflow-xyz PR #124,
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

No CID changes: every CID in this feature is `cid::seal(&Value)` on the JSON the command exported (the
identity runs' contract: sealed bytes are the document as written), and every parity vector of `tests/cid.rs`
and `tests/model.rs` is unchanged. `cid.rs` itself is not the file it was (§8): sealing is now bounded in work
(`[DOCUMENT_TOO_COMPLEX]`) and its recursion lives on the heap, which changes what a hostile document costs and
never what a sound one hashes to. `model_id.rs` gains one helper, `canonical_value` (§8).

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
message follows, and so does a number past 2^40 in magnitude: a capacity, an initial balance or an amount is
bounded because a pattern sums them and pflow-compose adds unchecked), `LINK_REFUSED` (pflow-compose's verdict
and the page rules, with their codes), `DOCUMENT_TOO_COMPLEX` (§1.3).

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
shapes the rule refuses. For form (a), `bundle.load`, `bundle.export`, `store.put` and `store.open` also report
`bundle_cid`, the seal of the plain Bundle that form flattens to (what a Bundle-reading tool such as pflow.xyz
or go-pflow would hash), **when asked**: `bundle_cid: true`. It is not computed otherwise, because it is the one
expensive seal of a composition (the arcs of a flattened saga are a long list: a hundred steps cost 8 to 11
seconds in a release build, two hundred 48 to 76, where the recipe seals in a moment), only a person
comparing with another tool wants it, and an answer that carries a field nobody asked for should not wait for
it. Without the argument the result has neither `bundle_cid` nor `bundle_cid_error`, and the flattened Bundle is
not sealed at all; with it the Bundle is sealed once by that command. A Bundle document is its own plain Bundle
(its `cid` is the seal), so the argument adds nothing for form (b). The CID a receipt names, the autosave
(`session.dump_state`) and the content checks seal the recipe only, whatever is asked, which is small however
large the net it builds. A Bundle too large to seal, asked for, gives `bundle_cid: null` and a
`bundle_cid_error` (`[DOCUMENT_TOO_COMPLEX] …`); the composition still loads and has its `cid`. Sealing is bounded: URDNA2015 is superlinear (a long `@list` is quadratic in its length, groups of
identical sibling objects are factorial), so `cid::seal` counts its steps and refuses a document that needs more
than `cid::seal_work_limit` with `[DOCUMENT_TOO_COMPLEX]`, the same on every platform. What an honest document
needs is about its blank nodes squared (a net's arcs are one long list; about 0.8 to 0.95 times the square of
the blank-node count, rising with size, for the nets of `examples/`: tic-tac-toe 0.4 million steps for 701 nodes,
connect-3 2.9 million for 1796, connect-4 37 million for 6260, a saga of 40 steps flattened 2.4 million), so the
limit is twice the square of the document's blank nodes, between 2 million (`MIN_SEAL_WORK`: what any small
document, however it is built, may spend) and 80 million (`MAX_SEAL_WORK`). A chain of 20,000 alike list cells
(`MAX_SEAL_DEPTH`) is refused as soon as the walk gets that deep, about a hundred thousand steps in, rather than
after the ceiling has been spent walking it; the guard has a test with the constant lowered and one with a real
list.
A raw Bundle that cannot be sealed cannot be given a CID, so `bundle.load` and `store.put` refuse it with that
code. A document is refused for its keys, its caps or its cites before it is sealed. A
PatternComposition has no part CIDs, so it is its own `@type` with CID = seal(doc); `tests/composition_documents.rs`
(`the_exported_documents_agree_with_the_identity_rule`, `the_by_reference_identity_vectors_load_to_their_golden_cids_and_export_unchanged`)
pins that `model_id(doc).sealed` equals the CID the commands report, for both forms, the cafe Bundle and the
four by-reference vectors of `tests/fixtures/model/`. A ModelContent CID is a handle for storing and
receipting that document, not the identity of a model. This feature changes no net's CID.

## 2. What each form can and cannot draw

The views read `bundle.state`, which now carries `document` (`form`, `origin`, `name`, `description`,
`preserved_keys`, `edited`; the CID is `bundle.load`'s and `bundle.export`'s), `capabilities`, `missing` (a
list of `{code, text}` written for a person), `content` (a summary) and, for a catalog-built composition only,
`params_note`. A view that cannot draw something shows the `missing` text instead.

| Capability | PatternComposition | Raw Bundle |
|---|---|---|
| Flatten, flat net, wires, fused groups, incidence, token game | yes | yes |
| `bundle.validate` report | yes | yes |
| Port schema colours, `pattern_port` ontology keys | yes | only if the subnets declare ports; no ontology keys (`NO_PATTERN`) |
| Snap-together candidates, `bundle.add_link` verdicts | yes | for the ports it declares, between two subnets that declare some (`snap_together`); cafe's `pantry` declares one (`beans`) and the other two none (`NO_PORTS`) |
| Idiom and cq/13 lighting, pool-leak flag (`idiom_of`) | yes (by pattern) | no (`NO_PATTERN`) |
| Operad holes, pattern hovers | yes | no |
| Scenario runs, token game (`bundle.scenario`, `bundle.step`), Flat chips, Process view | the scenarios its content declares (the same list for all of them) | same; none declared → `NO_SCENARIOS`, and the Process canvas says so rather than draw another model's runs |
| Lens test | the strict test, fixes and the commit/refund reading, all of which read a declared `commit` scenario as the forward pass | same; no `commit` declared → no verdict (the strict test would hold vacuously on an empty forward pass), said by cause: the view tells "does not flatten" from "declares no commit scenario" |
| Findings, notes, convention examples | its content, else `NO_CONTENT`; read by the panel (§3.6), and by the Flat view's conservation box | same |
| Links drawn between ports (Cospans, Spiders, Operad, String) | yes | only links whose ends are ports; links that end on places or transitions (cafe's seven) are not anchored there, and each of those views says how many in its caption and in the notes under it. Flat net, Boundaries, Laws and Incidence read them through Flatten |
| Layout | computed by the views (neither form stores positions: `NO_LAYOUT`) | same |

`missing` codes: `NO_PATTERN`, `NO_PORTS`, `NO_SCENARIOS`, `DORMANT` (content entries that name elements the
model does not have), `NO_CONTENT`, `NO_LAYOUT`, `FLATTEN_FAILED`, `KEYS_PRESERVED` (the document has keys
this build writes back but does not interpret; read from what the next export carries, so an edit that removes
the part or link holding a key drops it from the list).

The two forms are not interchangeable on export: a PatternComposition exported as a Bundle
(`bundle.export {form: "bundle"}`) drops each subnet's `pattern`/`params` recipe, so it cannot be rebuilt as
form (a) from that Bundle.

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
| `order` | no | integer (a string or a fraction is refused); sorts entries (the hash does not preserve array order, so authored order is a scalar) |
| `about` | yes, ≥ 1 | element ids (§3.2) the entry is attached to; model-wide is `["model:"]` |
| `views` | no | view ids that show it (`cospan spiders operad string flat boundaries lens incidence process laws`); `convention-example` needs exactly one |
| `cites` | no | `{repo, path, rev?, from?, to?, note?}` (a source cite: `repo` is a project name, `from`/`to` positive line numbers in order) or `{bib: "<key>"}` (a key of the ontology's `bib`, alone); anything else is `CITE_INVALID`; at most 64 |
| `check` | no | canonical-JSON string of a check (§3.4) |
| `scenario` | `scenario` kind only (on another kind: `CONTENT_FIELD`) | canonical-JSON string of `{script: ["<subnet>/<transition>", …], expect: [{place, op, n, says}], note?}`; `op` is `==` `!=` `<` `<=` `>` `>=` |

Any other key, `by` included, is `CONTENT_FIELD`: it would not survive an export. A top-level `@id` is accepted
when it is the document's own CID (what `store.get` returns) and an export drops it. `text` that is only
whitespace is no text.

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

`model:` takes no ref: `model:orders` is an id that names nothing (`ID_UNRESOLVED`), not a second way to say
"the whole model".

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
| `CHECK_REF_UNRESOLVED` | error under `strict`, warning otherwise | a check's `args.scenario` the content does not declare, or an `args.script` member the model lacks |
| `ID_SYNTAX` | error | not `<type>:<ref>`, or an unknown type |
| `SCENARIO_INVALID` | error | the `scenario` JSON does not parse, an unknown `op`, or (under `strict`) a member or place that is not in the net |
| `SCENARIO_UNRESOLVED` | warning, when not `strict` | a scenario's member or place the model lacks: the scenario is dormant but still listed and run, and stops where the member is missing (the prototype's behaviour, which the parity goldens depend on) |
| `CITE_INVALID`, `CITE_BIB_UNKNOWN` | error | a cite of the wrong shape (§3.1); a `bib` key the ontology does not have |
| `CONVENTION_DUPLICATE` | warning | a second `convention-example` for one view: both are drawn |
| `CHECK_INVALID`, `CHECK_COMMAND_FORBIDDEN` | error | §3.4 |
| `COMPOSITION_CID_STALE` | warning | `composition` names a CID other than the open composition's |
| `CONTENT_LIMIT` | error | more than 1000 entries; an entry with more than 32 KiB of text, a title over 512 bytes, more than 512 `about` ids or more than 64 cites; checks totalling more than 1024 steps; a scenario or a check step that fires more than 1000 members or expects more than 256 rows (a check's own 16-step cap is `CHECK_INVALID`; a document over 4 MiB is refused `[DOCUMENT_TOO_LARGE]` before it is read). `comp::run` itself stops at 1000 members, and `session.restore_state` applies these bounds to content it reads back |

The refusals around the document itself: `[COMPOSITION_FIELD]` (a form (a) key or value of the wrong shape, an
unknown key, an `@id` that is not the document's CID), `[COMPOSITION_LIMIT]` (more than 128 instances or
subnets, or 1000 links), `[DOCUMENT_CONTEXT]` (either form), `[DOCUMENT_TYPE]`, `[DOCUMENT_KIND]` (the store
keeps no document of that kind), `[BUNDLE_PARSE]` (pflow-compose cannot read it), `[CONTENT_REFUSED]` (content
that does not validate; its codes follow), `[CONTENT_SUBJECT_UNSUPPORTED]` (plan content cannot attach to a
composition), `[NO_CONTENT]`, `[NO_COMPOSITION]`.

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

* `command` must be one of `checks::READS`: the registered `bundle.` and `plan.` queries that read the
  model. `content.*`, every editing command, and the two queries that are not reads, `bundle.receipt` (it
  signs with the local receipt key) and `bundle.export` (it writes a document), are refused
  (`CHECK_COMMAND_FORBIDDEN`), so a document from outside cannot run anything but a read of the model. A test
  makes a new `bundle.*` / `plan.*` query a decision.
* Names a check uses must exist: `args.scenario` must be declared by the content, `args.script` members must
  be transitions of the model (`CHECK_REF_UNRESOLVED`). A document's checks run at most 1024 steps in all.
* A check runs on a clone and may not build a variant composition, so a finding about what a *different*
  wiring does (the pool-share configuration of `find-capacity-check-is-post-firing`) cannot carry one. The
  orders library's findings without a check are those whose evidence is not in the registry: an external
  checker's run (`find-chyp-proves-link-order-irrelevant`, `lens-agrees-with-gopflow`), a reading of a paper
  (`find-wiring-diagrams-are-undirected`), a drawing's token-identity rule (`process-steps-chain`), and the
  variant wiring above. A finding with no check is a claim the author makes, not one the registry decides.
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

`content.library` lists them. Library content is attached to a catalog-opened composition, and to any loaded
document (either form; `bundle.load {document}` or `{cid}`, `store.open`) whose exported CID is the one the
content was written against (the example, reloaded); any other loaded document (an edited example, a raw
Bundle) is bare unless the call passes `content`, and says so (`NO_CONTENT`).
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

**What the views read from content, and what they do not yet.** Read from the model, so a different model
draws its own or nothing: the declared scenarios (the Flat view's chips and token game, the Process view's
runs, `bundle.run`, the lens test), the model-content panel (convention example, findings, caveats,
judgments, notes), and the Flat view's "Conservation is not liveness" box (the entry
`flat-conservation-not-liveness`). Still written in the view code, per example (`*_orders` / `*_c4`), and
*not* read from content: the hover and caption prose of the ten views that names orders elements (the saga
and pool caveats of the Process view, the spiders' pool-leak hover, the Flat view's "why the worker does not
return" narration and idiom caveat, the Laws view's sync-spiders reading, the Cospans view's "Add payment2…"
hint), the prototype's per-view `convention.summary` / `convention.honesty` paragraphs, and the cites attached
to them. Most duplicate a content entry and should be read from it; that is the remaining work of D11, not
done here.

## 4. Commands

Arguments and results are JSON. `Json` arguments accept the value or a string holding it.

### `bundle.load` (edit, journaled, one undo step)

`document` (Json, a form (a) or (b) document) **or** `cid` (a stored document), optional `content`
(Json, a ModelContent document; or `library: "<id>"`), optional `strict` (default true), optional `bundle_cid`
(bool, default false: also seal the Bundle a form (a) document flattens to, §1.3).
Result: `{form: "composition"|"bundle", name, cid, bundle_cid? and bundle_cid_error? (only when asked), model_cid, reduced, parts, model_error?,
instances, links, validation, flatten: {ok, error?}, missing, content: {source, entries, kinds, unresolved,
warnings, dormant, scenarios}}`. `cid` is the seal of the document as written (as received, for a raw Bundle
and for a form (a) `params` string); `model_cid`, `reduced` and `parts` are `model_id`'s reading of it (§1.3).
Replaces any open composition.

### `bundle.export` (query, stores a Report artifact)

`form` (`auto` default: the form it was loaded or built as; `composition`, `bundle`), `content` (bool), `bundle_cid`
(bool, default false, as for `bundle.load`).
Result: `{form, document, cid, bundle_cid? and bundle_cid_error? (only when asked), model_cid, reduced, parts, content?: {document, cid}}`. `composition` from a
composition with any `pattern: ""` instance is refused `[EXPORT_NEEDS_PATTERNS]`. `bundle` from a
raw-loaded, unedited composition returns the loaded JSON; otherwise the Bundle pflow-compose serialises
(with `@context`/`@type`). The built-in orders example exports as form (a) and reloads identically.

### `content.*`

| Command | Journal | Does |
|---|---|---|
| `content.library` | no | `{library: [{id, subject, name, entries, checks, scenarios, anchors}]}` |
| `content.get` | no | `{source, subject, name, entries, total, dormant}` for the open composition (default) or `subject: "plan"`; filters `kind`, `about` (an element id), `view`; resolved entries only unless `all: true` |
| `content.load` | yes | `document` (Json) or `library`; `strict` (default true); attaches to the open composition. Result: the content summary of `bundle.state` plus `cid` (of the normalised document, §5) and `received_cid` when the document as received seals to another CID |
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

* `bundle.state` gains `document`, `capabilities`, `missing`, `content` (a summary) and, for a catalog-built
  composition, `params_note`. Link ends gain `place`/`transition` when a link names those instead of a port.
  Nothing is removed.
* `bundle.scenarios` returns the open composition's declared scenarios, else (no composition open) the
  `orders` library's; the element shape is unchanged. `bundle.run {scenario}`, `bundle.scenario {id}` and
  `bundle.step` resolve a declared scenario of the open composition (one it does not declare is refused,
  naming those it does).
* `store.put` takes `composition: true` (the open composition's document; `form` as in `bundle.export`),
  `content: true` (its content, stored with `about` = the composition's CID), or `document` (any
  document `store::kind_of` knows). `store.open` opens by kind; `store.list` shows `kind` and `about` and takes
  `about` to find the content stored for a composition. `store.put` and `store.open` take `bundle_cid` like
  `bundle.load` (§1.3).
* `file.open` of a PetriNetBundle, PatternComposition or ModelContent file is refused `[DOCUMENT_TYPE]` with a
  pointer to `bundle.load` / `content.load`; it opens nets.
* `provenance.verify_receipt` replays `composition-run`, `composition-check` and `composition-content` receipts (§6).
* `session.dump_state`'s `composition` part also carries `origin` (`catalog`, `document`, `bundle`): a
  catalog-built composition restores as the catalog's, so `bundle.state` keeps `params_note` across a save.
* See §8 for the observable changes that are decisions.

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
without the `@id` the store added, because the loaders refuse a key they do not know. The store is
addressed by content, so a read of a composition, a bundle or a content document is checked: `Store::get_model`
seals what it read and a file that seals to another CID is refused `[STORE_ALTERED]` ("a different document … the
store file was altered"), whether it was asked for by `store.get`, `store.open`, `bundle.load {cid}` or a receipt's
replay. **A net is not checked**: it is parsed, exactly as on main, because sealing one costs its size squared
(connect-4's takes tens of seconds) and every net the editor opens, and every `cid` part of a Bundle, is read
there; so a net file altered in place is served as it is (`replace_all`, which restores a saved state, still
verifies every file). The check costs one seal per store handle for the same bytes: the handle remembers the CID
and a digest of the file that sealed, so a second read, or `store.open` after `bundle.load {cid}`, is free, a
document the handle `put` is known to seal, and a changed file is sealed again; a refusal is never remembered.
A `cid` part of a Bundle whose file is there and not served is reported with the store's cause
(`[BUNDLE_PART_UNRESOLVED] subnets[0] refers to <cid>, which the store did not serve: [STORE_ALTERED] …`, or that
it is a composition and not a net), and "is not in the local store" is said only of a CID the store does not have.
`store.put` of one of these kinds over a file that does not seal to its name writes the document over it and says
so (`existed: true, repaired: true`); a net's file is left alone. `store.supersede` names the two kinds from the
index and reads no document. A document's own `@id`
other than its CID (a raw Bundle may carry one) is replaced by the CID in the store and not given back by
`store.open`; `store.put` says so.

A **content** document is stored as its normalised form (`check` and `scenario` as canonical text, nothing the
build does not read), the document `content.export` writes, so one CID names it everywhere: the store, `about`,
a receipt's `content`. `store.put` says what the document as received seals to when that differs, as
`content.load` does (`received_cid`). A document that does not parse as content is stored as given, with a
note, and `store.open` refuses it. **Plan content** (the Connect-4 library's) is library-only: `store.put`
accepts it as a document of kind content, but it can attach only to a composition, so `store.open` refuses it,
and `content.receipt` takes no subject, so its checks cannot be receipted. `replace_all` refuses
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
* **The labels are bound.** A replay runs the check or the script *in the receipt*, so once the content is
  found the receipt's check must be that entry's check there, and a run's script and expectations those of the
  declared scenario of that id; a difference is `bindingError` and the receipt is `invalid`. A run receipt whose
  content cannot be found stays `valid` and carries a `bindingNote` that its declaration is unchecked.
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
* `crates/app/tests/loaded_documents.rs`: the ten views on the cafe Bundle, with and without content: the
  Process and Flat views draw the scenarios it declares, a model with none says so, the port-anchored views say
  how many links they cannot anchor instead of claiming there are none, the lens says why it has no verdict, the
  Flat view's conservation box is read from content.
* `tests/composition_documents.rs` also pins the identity rules of §1.2-§1.3: the four by-reference vectors of
  `tests/fixtures/model/` load to their sealed and model CIDs and export unchanged, a sealed `params` string is
  never rewritten, an edit keeps the keys and parts it did not touch.
* The bounds and the guards (added after review): `cid.rs` unit tests (a long list is refused, not overflowed;
  identical siblings are refused; the work is counted; the limit grows with the document and the examples
  fit it; the depth guard refuses a chain the work budget would allow, with the constant lowered, and a list
  as long as the real one); `tests/cid.rs` (connect-3 still seals); `tests/composition_documents.rs` (a hostile document is
  `[DOCUMENT_TOO_COMPLEX]` and opens nothing; a document refused for its keys is not sealed first; a saga whose
  Bundle is too large to seal (the limit lowered for the test) still loads, and `dump_state` seals the recipe once; a parameter past 2^40; a
  replaced part does not keep the cid it replaced; the link rename and the raw Bundle's link cap round trip);
  `tests/model_content.rs` (the library's findings carry their checks, pinned; `model:x` does not resolve;
  scripts and expected rows are bounded; content refused is not sealed); `tests/composition_receipts.rs` (a store
  file of the composition, bundle and content kinds that is not its name's document is refused on every read
  and sealed once per handle for the same bytes; a net is read without a seal; `store.supersede` seals nothing; a
  bundle part the store refuses is reported with the cause; putting again repairs an altered file; a stored
  composition or content in another spelling is not the document a receipt names; a forged
  receipt is invalid without a replay for each kind; the receipt, verify and state commands never seal the
  flattened Bundle); `tests/composition_documents.rs` (`bundle_cid` is absent unless asked for and sealed once
  by the command that was); `tests/composition_store.rs` (a content document's `@id` must be its CID); `tests/session_state.rs` (a restored composition keeps its origin).
* The existing `views_*` and `compose` tests pass unchanged.

## 8. Decisions

Recorded because the acceptance criteria say what they relax.

1. **`model_id.rs` is not untouched.** It gains `pub fn canonical_value(&Value) -> String`, a wrapper over its
   private canonical writer, so the form (a) writer emits a recipe's `params` with the identity rule's own
   writer (§1.1) and cannot drift from it. Additive: `tests/model.rs`, `tests/fixtures/model/` and the cid fixtures
   are byte-identical to main and pass.
2. **`cid.rs` is bounded, not changed in what it computes.** The work counter, the `[DOCUMENT_TOO_COMPLEX]`
   refusal and the heap stack for the N-degree hash (§1.3) turn a document that took half a minute, or aborted
   the process on a long list, into an error. Every CID of `tests/cid.rs` and `tests/model.rs` is unchanged,
   and the nets of `examples/` still seal (connect-3 is a test; connect-4 needs 37 million steps and fits
   the limit its 6260 blank nodes give). A net or Bundle past 80 million steps (about 9000 blank nodes of
   arcs, which main sealed in minutes if its stack held) is refused and cannot be given a CID.
3. **Existing commands that now read the open composition's content.** `bundle.scenarios`, `bundle.run`,
   `bundle.scenario`, `bundle.step`, `bundle.game` and the lens read the open composition's declared scenarios;
   main always used the orders four. A composition without them has none (`NO_SCENARIOS` in `missing`), and the
   lens test gives no verdict. `bundle.run`'s unknown-scenario error says "is declared for this model" and names
   those it does declare.
4. **`store.list` rows and the table artifact gain `kind` and `about`.** `kind` is the third column, before
   `places`, and `about` the last: a consumer that reads the table by column position must be checked. A net's
   index entry is byte-for-byte what it was.
5. **The Flat view's "Conservation is not liveness" sentence is the content entry's static text.** The live
   value `({v})` and the ROADMAP line cites that main appended to the paragraph remain in the hover only.
6. **`file.open` of a PetriNetBundle, PatternComposition or ModelContent file is refused `[DOCUMENT_TYPE]`**
   (with the command to use) where main opened it as an empty net.
7. **Parity goldens are unchanged**: `parity/composition-views/goldens.json` and the `views_*` / `compose` tests pass
   without edits; the content panel is outside the manifest.
8. **`store.get`, `store.open` and a bundle's `cid` parts check what they read, for the new kinds only.** On main
   `Store::get_model` only parsed the file (only `replace_all` verified, when a saved state is restored); here a
   composition, a bundle or a content document that does not seal to its name is refused `[STORE_ALTERED]`, once per
   store handle for the same bytes (§5). That is new behaviour of an existing command, recorded because it changes
   what a reader of the store can be told. A net is read exactly as on main: no check, no seal, and a net's index
   entry is byte-for-byte what it was. **A forged `resultHash` or signature is invalid without a replay**: that half
   was already true of the code and is now pinned by tests.
9. **`bundle_cid` is opt-in** (`bundle_cid: true` on `bundle.load`, `bundle.export`, `store.put`, `store.open`). Its
   seal of the flattened Bundle was the slowest thing a plain load did (§1.3), and nothing but a comparison with
   another tool reads it. The commands are new on this branch, so nothing that read `bundle_cid` by default
   existed outside it; the tests that assert it ask for it.
