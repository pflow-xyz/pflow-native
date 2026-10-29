//! What a stored document is, by its `@type` (docs/composition-documents.md
//! §5). The store keeps every kind under its pflow seal in `models/`; the
//! index says which kind, so a net's entry stays byte-for-byte what it was
//! (a net has no `kind` key).

use serde_json::Value;

use crate::composition::document::refuse;
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    /// A Petri net in editor JSON-LD ("Shape A").
    Net,
    /// A raw `PetriNetBundle`.
    Bundle,
    /// A `PatternComposition`.
    Composition,
    /// A `ModelContent` document.
    Content,
    Unknown,
}

impl DocKind {
    pub fn name(self) -> &'static str {
        match self {
            DocKind::Net => "net",
            DocKind::Bundle => "bundle",
            DocKind::Composition => "composition",
            DocKind::Content => "content",
            DocKind::Unknown => "unknown",
        }
    }

    /// The `kind` an index entry records: absent for a net.
    pub fn index_name(self) -> Option<&'static str> {
        (self != DocKind::Net).then(|| self.name())
    }

    pub fn from_index(kind: Option<&str>) -> DocKind {
        match kind {
            None | Some("net") => DocKind::Net,
            Some("bundle") => DocKind::Bundle,
            Some("composition") => DocKind::Composition,
            Some("content") => DocKind::Content,
            Some(_) => DocKind::Unknown,
        }
    }
}

/// Classifies a document by its `@type`. An object with `places` and
/// `transitions` and no other kind's `@type` is a net: untyped (the legacy
/// editor shape) or typed by its net type (`ResourceNet`, as
/// examples/producer-consumer.json), which `Document` keeps and the store
/// has always sealed like any other net.
pub fn kind_of(v: &Value) -> DocKind {
    match v.get("@type").and_then(Value::as_str) {
        Some("PetriNet") => DocKind::Net,
        Some("PetriNetBundle") => DocKind::Bundle,
        Some("PatternComposition") => DocKind::Composition,
        Some("ModelContent") => DocKind::Content,
        _ if v.get("places").is_some() && v.get("transitions").is_some() => DocKind::Net,
        _ => DocKind::Unknown,
    }
}

/// [`kind_of`], with `[DOCUMENT_KIND]` for a document the store does not
/// keep.
pub fn require_kind(v: &Value) -> Result<DocKind> {
    match kind_of(v) {
        DocKind::Unknown => {
            let ty = match v.get("@type") {
                Some(Value::String(t)) => format!("@type {t:?}"),
                Some(other) => format!("@type {other}"),
                None => "no @type".into(),
            };
            Err(refuse(
                "DOCUMENT_KIND",
                format!(
                    "the store keeps a PetriNet, a PetriNetBundle, a PatternComposition or a ModelContent document; this one has {ty}"
                ),
            ))
        }
        k => Ok(k),
    }
}
