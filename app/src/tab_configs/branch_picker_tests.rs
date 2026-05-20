use warpui::keymap::macros::*;

use super::{build_keymap_context, BRANCH_PICKER_COLLAPSED};

#[test]
fn keymap_context_advertises_collapsed_flag_when_closed() {
    // A closed, focused picker must expose the flag so its Space binding
    // fires and opens the dropdown (issue #11138).
    let context = build_keymap_context(false);
    assert!(
        context.set.contains(BRANCH_PICKER_COLLAPSED),
        "collapsed picker must expose {BRANCH_PICKER_COLLAPSED}; got {:?}",
        context.set,
    );
}

#[test]
fn keymap_context_omits_collapsed_flag_when_expanded() {
    // While expanded, the dropdown's filter editor is focused; the picker
    // must NOT expose the flag, or Space would be stolen from the editor
    // instead of typing a literal space into the filter query.
    let context = build_keymap_context(true);
    assert!(
        !context.set.contains(BRANCH_PICKER_COLLAPSED),
        "expanded picker must not expose {BRANCH_PICKER_COLLAPSED}; got {:?}",
        context.set,
    );
}

#[test]
fn space_binding_predicate_matches_only_collapsed_picker() {
    // The Space fixed binding is scoped to id!(BRANCH_PICKER_COLLAPSED): it
    // must match a collapsed picker's context and miss an expanded one.
    let predicate = id!(BRANCH_PICKER_COLLAPSED);
    assert!(
        predicate.eval(&build_keymap_context(false)),
        "Space must toggle a focused, collapsed picker",
    );
    assert!(
        !predicate.eval(&build_keymap_context(true)),
        "Space must not fire while the picker's filter editor is focused",
    );
}
