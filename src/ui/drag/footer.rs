use gds_uim::el;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::{IntoElement, prelude::*, *};

use super::{FOOTER_PADDING_BOTTOM, PADDING_X};
use crate::{app::AppExt as _, appearance::tokens::DataListStyle, ui::container::container_traits};

/// Muted footer line of a [`DragCard`](super::DragCard), e.g. overflow count.
#[derive(IntoElement)]
pub struct DragCardFooter {
    base: Div,
}

impl Default for DragCardFooter {
    fn default() -> Self {
        Self {
            base: el! { .div { flex; items_center; min_w_0; truncate; px: PADDING_X.px; pb: FOOTER_PADDING_BOTTOM.px; } },
        }
    }
}

container_traits!(DragCardFooter);

impl RenderOnce for DragCardFooter {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let style = std::mem::take(self.base.style());
        el! {
            $(self.base) {
                text_color: $(cx.theme().muted_foreground);
                text_size: $(cx.gds().appearance().typography().scaled(DataListStyle::META_SIZE));
                refine_style: $&style;
            }
        }
    }
}
