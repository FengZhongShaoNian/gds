use zgui::custom::{
    Custom, CustomElement, CustomLayoutCx, CustomMeasured, ScenePainter, Space, custom,
};
use zgui::elements::Element;

/// Custom `zgui` retained-scene grid renderer for terminal cells, selection, and cursor.
#[derive(Default)]
pub(super) struct TerminalGrid;

impl TerminalGrid {
    pub(super) fn element() -> Element<Custom> {
        let (element, _handle) = custom(Self);
        element
    }
}

impl CustomElement for TerminalGrid {
    fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
        let width = cx.known_width.unwrap_or(match cx.available.0 {
            Space::Definite(w) => w,
            _ => 0.0,
        });
        let height = cx.known_height.unwrap_or(match cx.available.1 {
            Space::Definite(h) => h,
            _ => 0.0,
        });
        CustomMeasured {
            width,
            height,
            ..CustomMeasured::default()
        }
    }

    fn paint(&mut self, _painter: &mut ScenePainter<'_>) {}
}
