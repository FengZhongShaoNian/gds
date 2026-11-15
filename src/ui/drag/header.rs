use gds_uim::el;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::{FontWeight, IntoElement, prelude::*, *};

use super::{HEADER_GAP, HEADER_PADDING_Y, PADDING_X};
use crate::{app::AppExt as _, appearance::tokens::DataListStyle, ui::container::container_traits};

/// Top summary strip of a [`DragCard`](super::DragCard), showing icon and item count.
#[derive(IntoElement)]
pub struct DragCardHeader {
    base: Div,
}

impl Default for DragCardHeader {
    fn default() -> Self {
        Self {
            base: el! { .div { flex; items_center; gap: HEADER_GAP.px; min_w_0; px: PADDING_X.px; py: HEADER_PADDING_Y.px; border_b_1; font_weight: FontWeight::SEMIBOLD; } },
        }
    }
}

container_traits!(DragCardHeader);

impl RenderOnce for DragCardHeader {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let style = std::mem::take(self.base.style());
        el! {
            $(self.base) {
                border_color: $(cx.theme().border);
                text_size: $(cx.gds().appearance().typography().scaled(DataListStyle::META_SIZE));
                refine_style: $&style;
            }
        }
    }
}
