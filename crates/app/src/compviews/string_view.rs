//! The string view (see `docs/composition-views-scene.md`): the pattern level
//! (this view's own drawing: `b_orders`, `b_c4`) and the transition level
//! (diagram-viewer.js's drawing of the flat net or the Connect-4 net:
//! `b_transition`), switched by `view.composition {level}`.

use super::*;

pub(super) fn string_orders(c: &mut Ctx, area: Rect) {
    if c.level == "transition" {
        b_transition::orders(c, area);
    } else {
        let area = b_transition::toggle(c, area, true);
        b_orders::draw(c, area);
    }
}

pub(super) fn string_c4(c: &mut Ctx, area: Rect) {
    if c.level == "transition" {
        b_transition::c4(c, area);
    } else {
        let area = b_transition::toggle(c, area, false);
        b_c4::draw(c, area);
    }
}
