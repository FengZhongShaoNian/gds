//! Compact floating preview card for multi-item drag payloads.

mod content;
mod footer;
mod header;

pub use content::DragCardContent;
pub use footer::DragCardFooter;
pub use header::DragCardHeader;

use gds_uim::{component, el};
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, ThemeStyled as _};
use gpui_kit::{IntoElement, prelude::*, *};

use super::container::container_traits;

const MIN_WIDTH: f32 = 140.;
const MAX_WIDTH: f32 = 248.;
const PADDING_X: f32 = 10.;
const HEADER_PADDING_Y: f32 = 6.;
const HEADER_GAP: f32 = 8.;
const CONTENT_PADDING_Y: f32 = 6.;
const CONTENT_GAP: f32 = 3.;
const FOOTER_PADDING_BOTTOM: f32 = 6.;

/// Floating preview card that follows the pointer during a multi-item drag.
#[component]
#[derive(IntoElement)]
pub struct DragCard {
    /// Pointer grab offset supplied by GPUI's `on_drag`, keeping the pointer
    /// itself clear over the drop target.
    #[attr(option)]
    offset: Option<Point<Pixels>>,
    base: Div,
}

impl DragCard {
    /// Distance from the pointer tip to the card's top-left corner.
    pub const POINTER_GAP: f32 = 12.;
}

impl Default for DragCard {
    fn default() -> Self {
        Self {
            offset: None,
            base: el! { .div { flex; flex_col; min_w: MIN_WIDTH.px; max_w: MAX_WIDTH.px; overflow_hidden; border_1; } },
        }
    }
}

container_traits!(DragCard);

impl RenderOnce for DragCard {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let style = std::mem::take(self.base.style());
        let card = el! {
            $(self.base) {
                popover_style: cx;
                border_color: $(cx.theme().border);
                font_family: $(cx.theme().font_family.clone());
                refine_style: $&style;
            }
        };
        match self.offset {
            Some(offset) => {
                let position = offset + point(px(Self::POINTER_GAP), px(Self::POINTER_GAP));
                el! { .div { pl: $position.x; pt: $position.y; } [card] }.into_any_element()
            }
            None => card.into_any_element(),
        }
    }
}
