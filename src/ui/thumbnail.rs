//! Reusable still preview backed by `<image>` / `zgui_image::ImageBytes` and icon fallback.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use super::file_inspection::{Cache, State};
use crate::service::file_inspection::{Kind, Request, Source};
use crate::ui::icon::{IconName, IconProps};

fn fallback_icon(kind: Kind) -> IconName {
    match kind {
        Kind::Image => IconName::Image,
        Kind::Video => IconName::FileVideoCamera,
        Kind::Audio => IconName::FileMusic,
        _ => IconName::File,
    }
}

/// Renders a media thumbnail for `source`, falling back to a typed icon when unavailable.
#[component]
pub fn Thumbnail(
    cache: Cache,
    #[prop(into)] source: Signal<Source, LocalStorage>,
    #[prop(default = false)] show_status: bool,
    #[prop(into, optional)] class: Classes,
) -> impl IntoView {
    let _ = show_status;
    crate::ui::styles::install_ui_stylesheets();
    view! {
        box(class = "gds-thumbnail", class = class) {
            {move || {
                let src = source.get();
                let snap = cache.snapshot(&src, Request::Thumbnail);
                match snap.thumbnail {
                    State::Ready(bytes) => {
                        AnyView::new(view! { image(class = "gds-thumbnail__img", src = {Some(bytes.url())}) })
                    }
                    _ if src.kind == Kind::Image => {
                        let path_str = src.path.to_string_lossy().into_owned();
                        AnyView::new(view! { image(class = "gds-thumbnail__img", src = {Some(path_str)}) })
                    }
                    _ => AnyView::new(view! { Icon(name = fallback_icon(src.kind), size = 32.0) }),
                }
            }}
        }
    }
}
