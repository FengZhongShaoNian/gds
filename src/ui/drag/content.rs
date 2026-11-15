use gds_uim::el;
use gpui_kit::component::StyledExt as _;
use gpui_kit::{IntoElement, prelude::*, *};

use super::{CONTENT_GAP, CONTENT_PADDING_Y, PADDING_X};
use crate::{app::AppExt as _, appearance::tokens::DataListStyle, ui::container::container_traits};

/// Body of a [`DragCard`](super::DragCard) listing previewed dragged items.
#[derive(IntoElement)]
pub struct DragCardContent {
    base: Div,
}

impl Default for DragCardContent {
    fn default() -> Self {
        Self {
            base: el! { .div { flex; flex_col; gap: CONTENT_GAP.px; min_w_0; px: PADDING_X.px; py: CONTENT_PADDING_Y.px; } },
        }
    }
}

container_traits!(DragCardContent);

impl RenderOnce for DragCardContent {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let style = std::mem::take(self.base.style());
        el! {
            $(self.base) {
                text_size: $(cx.gds().appearance().typography().scaled(DataListStyle::NAME_SIZE));
                refine_style: $&style;
            }
        }
    }
}
