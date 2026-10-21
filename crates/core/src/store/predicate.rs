//! sim.pflow.xyz's predicate registry (`pkg/modelstore/predicate.go`),
//! copied name for name so an edge recorded here means what the same edge
//! means on sim: its axis (asserted vs proved), the entity kinds it
//! relates, its algebra (inverse, transitive, symmetric) and which tool may
//! record it.

use serde_json::{json, Value};

/// A content-addressed entity kind. The local store holds models,
/// collections, maps and relations; the other kinds exist on sim and are
/// named here so domains and ranges read as sim's do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Model,
    Collection,
    Prompt,
    Artifact,
    Map,
    Binding,
    Relation,
    System,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Model => "model",
            Kind::Collection => "collection",
            Kind::Prompt => "prompt",
            Kind::Artifact => "artifact",
            Kind::Map => "map",
            Kind::Binding => "binding",
            Kind::Relation => "relation",
            Kind::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Asserted, unverified; offered for migration on supersession.
    Taxonomic,
    /// A morphism; exists only with its verifier's witness.
    Structural,
}

impl Axis {
    pub fn name(self) -> &'static str {
        match self {
            Axis::Taxonomic => "taxonomic",
            Axis::Structural => "structural",
        }
    }
}

/// What an edge's object may be.
#[derive(Debug, Clone, Copy)]
pub enum Range {
    Kinds(&'static [Kind]),
    /// A term of a literal vocabulary (`ModelKind` for `@type`).
    Literal(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Predicate {
    pub name: &'static str,
    pub axis: Axis,
    pub domain: &'static [Kind],
    pub range: Range,
    pub inverse: Option<&'static str>,
    /// Set on an inverse-only name: never stored, read as this backwards.
    pub stored_as: Option<&'static str>,
    pub transitive: bool,
    pub symmetric: bool,
    /// Tools that may record it (sim's tool names).
    pub written_by: &'static [&'static str],
    pub comment: &'static str,
}

impl Predicate {
    pub fn writable_by(&self, tool: &str) -> bool {
        self.written_by.contains(&tool)
    }

    /// Writable through `store.link` (sim_link).
    pub fn linkable(&self) -> bool {
        self.stored_as.is_none() && self.writable_by(WRITER_LINK)
    }

    pub fn to_json(&self) -> Value {
        let range = match self.range {
            Range::Kinds(k) => json!({"kinds": kinds_json(k)}),
            Range::Literal(l) => json!({"literal": l}),
        };
        json!({
            "name": self.name,
            "axis": self.axis.name(),
            "domain": kinds_json(self.domain),
            "range": range,
            "inverse": self.inverse,
            "storedAs": self.stored_as,
            "transitive": self.transitive,
            "symmetric": self.symmetric,
            "writtenBy": self.written_by,
            "linkable": self.linkable(),
            "comment": self.comment,
        })
    }
}

fn kinds_json(k: &[Kind]) -> Vec<&'static str> {
    k.iter().map(|k| k.name()).collect()
}

pub const HAS_MEMBER: &str = "hasMember";
pub const TYPE: &str = "@type";
pub const SUPERSEDED_BY: &str = "supersededBy";
pub const MIGRATED_FROM: &str = "migratedFrom";

/// sim's tool names in `writtenBy`: the free-form link tool, the
/// supersession tool, the migration tool and the structural prover.
pub const WRITER_LINK: &str = "sim_link";
pub const WRITER_SUPERSEDE: &str = "sim_supersede_model";
pub const WRITER_ACCEPT: &str = "sim_accept_migration";
pub const WRITER_PROVE: &str = "sim_prove";

/// `/ns/models/v1`'s reviewed model kinds: `@type`'s literal range.
pub const MODEL_KINDS: &[&str] = &[
    "ArrivalProcess",
    "ConservedResourceCycle",
    "HasFungibleFamily",
    "QueueingSystem",
    "ResourcePool",
    "StructurallyDeadlockable",
    "UnclassifiedModel",
];

use Kind::*;

/// sim's frozen 12b set: what hasMember and cites relate.
const LINKABLE: &[Kind] = &[
    Model, Collection, Prompt, Artifact, Map, Binding, Relation, System,
];
const MODEL_COLLECTION_SYSTEM: &[Kind] = &[Model, Collection, System];
const ONLY_MODEL: &[Kind] = &[Model];

const fn taxonomic(
    name: &'static str,
    domain: &'static [Kind],
    range: Range,
    written_by: &'static [&'static str],
    comment: &'static str,
) -> Predicate {
    Predicate {
        name,
        axis: Axis::Taxonomic,
        domain,
        range,
        inverse: None,
        stored_as: None,
        transitive: false,
        symmetric: false,
        written_by,
        comment,
    }
}

const fn structural(
    name: &'static str,
    transitive: bool,
    symmetric: bool,
    comment: &'static str,
) -> Predicate {
    Predicate {
        name,
        axis: Axis::Structural,
        domain: ONLY_MODEL,
        range: Range::Kinds(ONLY_MODEL),
        inverse: None,
        stored_as: None,
        transitive,
        symmetric,
        written_by: &[WRITER_PROVE],
        comment,
    }
}

/// Every registered predicate, in sim's registry order.
pub const REGISTRY: &[Predicate] = &[
    taxonomic(HAS_MEMBER, &[Collection], Range::Kinds(LINKABLE), &[WRITER_LINK],
        "a collection's member; membership is an edge so the collection's own content id never changes when something joins it"),
    taxonomic(TYPE, ONLY_MODEL, Range::Literal("ModelKind"), &[WRITER_LINK],
        "a reviewed model kind from /ns/models/v1"),
    Predicate {
        inverse: Some("narrower"),
        transitive: true,
        ..taxonomic("broader", MODEL_COLLECTION_SYSTEM, Range::Kinds(MODEL_COLLECTION_SYSTEM), &[WRITER_LINK],
            "the subject is a narrower case of the object, as a human classified it")
    },
    Predicate {
        inverse: Some("broader"),
        stored_as: Some("broader"),
        transitive: true,
        ..taxonomic("narrower", MODEL_COLLECTION_SYSTEM, Range::Kinds(MODEL_COLLECTION_SYSTEM), &[],
            "the inverse reading of broader; never stored, link broader the other way round")
    },
    Predicate {
        symmetric: true,
        ..taxonomic("related", MODEL_COLLECTION_SYSTEM, Range::Kinds(MODEL_COLLECTION_SYSTEM), &[WRITER_LINK],
            "an associative link, read the same in either direction")
    },
    Predicate {
        symmetric: true,
        ..taxonomic("closeMatch", ONLY_MODEL, Range::Kinds(ONLY_MODEL), &[WRITER_LINK],
            "a human claim that two models represent the same kind of system; never a proof (isomorphicTo is the proved form)")
    },
    taxonomic("cites", LINKABLE, Range::Kinds(LINKABLE), &[WRITER_LINK],
        "the subject draws on the object without being derived from it (derivation is lineage)"),
    taxonomic("about", &[Artifact, Prompt, Map, Collection], Range::Kinds(&[Model, Collection, Binding, System]), &[WRITER_LINK],
        "a report, dataset or table concerning a model, collection, binding or system"),
    taxonomic("from", &[Binding], Range::Kinds(ONLY_MODEL), &["sim_bind"],
        "a binding's source model; recorded by sim_bind with the binding itself"),
    taxonomic("to", &[Binding], Range::Kinds(ONLY_MODEL), &["sim_bind"],
        "a binding's target model; recorded by sim_bind with the binding itself"),
    taxonomic("feeds", ONLY_MODEL, Range::Kinds(ONLY_MODEL), &["sim_bind"],
        "one model's output port is bound to the other's input; recorded by sim_bind"),
    taxonomic("produced", &[Prompt], Range::Kinds(&[Model, Artifact]), &["sim_prompt", "sim_reroll"],
        "the result a prompt actually returned"),
    Predicate {
        transitive: true,
        ..taxonomic(SUPERSEDED_BY, ONLY_MODEL, Range::Kinds(ONLY_MODEL), &[WRITER_SUPERSEDE],
            "old version → its replacement, recorded by store.supersede (sim_supersede_model)")
    },
    structural("isomorphicTo", true, true,
        "the same net up to renaming (coloured-net-v1); stored with the smaller id as subject"),
    structural("subnetOf", true, false,
        "the subject embeds injectively in the object, arcs induced (induced-incidence-v1)"),
    Predicate {
        transitive: true,
        ..taxonomic(MIGRATED_FROM, &[Relation], Range::Kinds(&[Relation]), &[WRITER_ACCEPT],
            "a taxonomic edge on a successor model → the edge on a superseded version it was carried forward from; recorded only by store.accept_migration")
    },
    structural("preservesInvariant", false, false,
        "the subject embeds in the object along a map under which every P-invariant of the object pulls back (invariant-pullback-v1)"),
    structural("refines", false, false,
        "every firing sequence of the subject, silent transitions dropped, is one of the object (id-simulation-v1)"),
];

pub fn lookup(name: &str) -> Option<&'static Predicate> {
    REGISTRY.iter().find(|p| p.name == name)
}

/// Names `store.link` writes, sorted (as sim's linkableNames).
pub fn linkable_names() -> String {
    let mut names: Vec<&str> = REGISTRY
        .iter()
        .filter(|p| p.linkable())
        .map(|p| p.name)
        .collect();
    names.sort_unstable();
    names.join(", ")
}

/// Every registered name, sorted.
pub fn registered_names() -> String {
    let mut names: Vec<&str> = REGISTRY.iter().map(|p| p.name).collect();
    names.sort_unstable();
    names.join(", ")
}

pub fn is_symmetric(name: &str) -> bool {
    lookup(name).is_some_and(|p| p.symmetric)
}

pub fn is_transitive(name: &str) -> bool {
    lookup(name).is_some_and(|p| p.transitive)
}

/// "taxonomic", "structural" or "unregistered".
pub fn axis_of(name: &str) -> &'static str {
    lookup(name).map_or("unregistered", |p| p.axis.name())
}

/// What supersession does to an edge of this predicate (sim's
/// `migrationFate`): offered = linkable; otherwise it stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    Offered,
    ToolWritten,
    Structural,
    Unregistered,
}

impl Fate {
    pub fn of(pred: &str) -> Fate {
        match lookup(pred) {
            None => Fate::Unregistered,
            Some(p) if p.axis == Axis::Structural => Fate::Structural,
            Some(p) if p.linkable() => Fate::Offered,
            Some(_) => Fate::ToolWritten,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Fate::Offered => "offered",
            Fate::ToolWritten => "tool-written",
            Fate::Structural => "structural",
            Fate::Unregistered => "unregistered",
        }
    }
}

/// "a model", "a model or a collection", "an artifact, prompt, map or collection".
pub fn kind_list(kinds: &[Kind]) -> String {
    let names: Vec<&str> = kinds.iter().map(|k| k.name()).collect();
    match names.as_slice() {
        [] => "nothing".into(),
        [a] => format!("{} {a}", article(a)),
        [a, b] => format!("{} {a} or {} {b}", article(a), article(b)),
        [first, .., last] => format!(
            "{} {} or {last}",
            article(first),
            names[..names.len() - 1].join(", ")
        ),
    }
}

pub fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_sims_shape() {
        assert_eq!(REGISTRY.len(), 18);
        assert_eq!(
            linkable_names(),
            "@type, about, broader, cites, closeMatch, hasMember, related"
        );
        assert_eq!(lookup("narrower").unwrap().stored_as, Some("broader"));
        assert_eq!(Fate::of("hasMember"), Fate::Offered);
        assert_eq!(Fate::of("supersededBy"), Fate::ToolWritten);
        assert_eq!(Fate::of("subnetOf"), Fate::Structural);
        assert_eq!(Fate::of("knows"), Fate::Unregistered);
        assert_eq!(kind_list(&[Model]), "a model");
        assert_eq!(kind_list(&[Model, Collection]), "a model or a collection");
        assert_eq!(
            kind_list(&[Artifact, Prompt, Map, Collection]),
            "an artifact, prompt, map or collection"
        );
    }
}
