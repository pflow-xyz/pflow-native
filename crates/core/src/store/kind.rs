//! What a stored document is, by its `@type` (docs/composition-documents.md
//! §5). The store keeps every kind under its pflow seal in `models/`; the
//! index says which kind, so a net's entry stays byte-for-byte what it was
//! (a net has no `kind` key).

use serde_json::Value;

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

/// Classifies a document by its `@type`; an untyped object with `places`
/// and `transitions` is a net (the legacy editor shape).
pub fn kind_of(v: &Value) -> DocKind {
    match v.get("@type").and_then(Value::as_str) {
        Some("PetriNet") => DocKind::Net,
        Some("PetriNetBundle") => DocKind::Bundle,
        Some("PatternComposition") => DocKind::Composition,
        Some("ModelContent") => DocKind::Content,
        None if v.get("places").is_some() && v.get("transitions").is_some() => DocKind::Net,
        _ => DocKind::Unknown,
    }
}
