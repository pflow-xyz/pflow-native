//! pflow-rs line cites for the composition views, from the one verified
//! table in core (`search_plan::pflow_rs_cite`, `pflow_rs_cites.json`), so
//! no view carries a line number of its own: they are re-checked against the
//! pinned revision by a core test.

use pflow_native_core::search_plan as sp;

/// The label of cite `key` in `cite_label`'s format, e.g.
/// `pflow-rs crates/pflow-metamodel/src/game.rs:1046-1179 @aacb7eb (Solver::search: ...)`.
///
/// Keys are the table's: `<prototype-file>.<name>` (`lens.value`,
/// `plan.driversFlags`), `piece.<name>[.N]` and `slot.<id>[.N]`. An unknown
/// key gives a label that says so, never a made-up line number.
pub fn pflow_rs(key: &str) -> String {
    match sp::pflow_rs_cite(key) {
        Some(c) => c.label(),
        None => format!(
            "pflow-rs ?{key} @{} (no such cite in pflow_rs_cites.json)",
            sp::PLAN_RS_REV
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::pflow_rs;

    #[test]
    fn a_known_key_reads_like_cite_label() {
        assert_eq!(
            pflow_rs("plan.search"),
            "pflow-rs crates/pflow-metamodel/src/game.rs:1046-1179 @aacb7eb \
             (Solver::search: fail-soft alpha-beta with the transposition table)"
        );
    }

    #[test]
    fn an_unknown_key_is_marked() {
        let s = pflow_rs("nope.nothing");
        assert!(
            s.contains("?nope.nothing") && s.contains("no such cite"),
            "{s}"
        );
    }
}
