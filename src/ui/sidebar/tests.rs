use super::SidebarPresentation;
use crate::appearance::tokens;

#[test]
fn sidebar_presentation_interpolates_progress_cleanly() {
    let p = SidebarPresentation::from_width(
        tokens::COLLAPSED_SIDEBAR_WIDTH,
        tokens::SIDEBAR_WIDTH,
        tokens::COLLAPSED_SIDEBAR_WIDTH,
    );
    assert_eq!(p.progress(), 0.0);
    assert!(!p.actions_visible());
}
