//! Reusable still preview backed by `<image>` / `zgui_image::ImageBytes` and icon fallback.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, css, view};

use super::file_inspection::{Cache, State};
use crate::app::Gds;
use crate::service::file_inspection::{Kind, Request, Source};
use crate::ui::icon::{IconName, IconProps};

const SHEET: &str = css!(
    r#"
    .gds-thumbnail {
        display: flex;
        align-items: center;
        justify-content: center;
        position: relative;
        overflow: hidden;
    }

    .gds-thumbnail__img {
        width: 100%;
        height: 100%;
        object-fit: contain;
    }

    .gds-thumbnail__fallback {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 6px;
        padding: 8px;
        color: var(--gds-muted-foreground);
    }

    .gds-thumbnail__status {
        font-size: 11px;
        color: var(--gds-muted-foreground);
        text-align: center;
        overflow-wrap: anywhere;
    }
    "#
);

fn fallback_icon(kind: Kind) -> IconName {
    match kind {
        Kind::Image => IconName::Image,
        Kind::Video => IconName::FileVideoCamera,
        Kind::Audio => IconName::FileMusic,
        Kind::Directory => IconName::Folder,
        _ => IconName::File,
    }
}

fn status_label(key: &'static str, fallback: &'static str) -> String {
    Gds::try_current().map_or_else(|| fallback.to_owned(), |gds| gds.i18n().text(key))
}

/// Renders a media thumbnail for `source` using cached `zgui_image::ImageBytes::url()`, direct
/// `<image src=...>` for file paths when uncached/static, and a typed [`Icon`] fallback.
#[component]
pub fn Thumbnail(
    cache: Cache,
    #[prop(into)] source: Signal<Source, LocalStorage>,
    #[prop(default = false)] show_status: bool,
    #[prop(into, optional)] class: Classes,
) -> impl IntoView {
    if zgui::view::current_host().is_some() {
        crate::ui::styles::install_ui_stylesheets();
        zgui::view::install_stylesheet("gds-ui-thumbnail", SHEET);
    }

    view! {
        box(class = "gds-thumbnail", class = class) {
            {move || {
                let src = source.get();
                let snap = cache.snapshot(&src, Request::Thumbnail);
                match snap.thumbnail {
                    State::Ready(bytes) => {
                        AnyView::new(view! {
                            image(class = "gds-thumbnail__img", src = {Some(bytes.url())})
                        })
                    }
                    State::NotRequested | State::Unavailable if src.kind == Kind::Image => {
                        let path_str = src.path.to_string_lossy().into_owned();
                        AnyView::new(view! {
                            image(class = "gds-thumbnail__img", src = {Some(path_str)})
                        })
                    }
                    State::Loading => {
                        let status = show_status.then(|| {
                            status_label("gds-thumbnail-loading", "Loading preview…")
                        });
                        AnyView::new(view! {
                            column(class = "gds-thumbnail__fallback") {
                                Icon(name = fallback_icon(src.kind), size = 32.0) {}
                                {status.map(|label| view! { text(class = "gds-thumbnail__status") {{label}} }.into_view())}
                            }
                        })
                    }
                    State::TooLarge => {
                        let status = show_status.then(|| {
                            status_label("gds-thumbnail-too-large", "This image is too large to preview")
                        });
                        AnyView::new(view! {
                            column(class = "gds-thumbnail__fallback") {
                                Icon(name = IconName::ImageOff, size = 32.0) {}
                                {status.map(|label| view! { text(class = "gds-thumbnail__status") {{label}} }.into_view())}
                            }
                        })
                    }
                    State::Failed(_) | State::Unavailable | State::NotRequested => {
                        let status = show_status.then(|| {
                            status_label("gds-thumbnail-failed", "Preview unavailable")
                        });
                        AnyView::new(view! {
                            column(class = "gds-thumbnail__fallback") {
                                Icon(name = fallback_icon(src.kind), size = 32.0) {}
                                {status.map(|label| view! { text(class = "gds-thumbnail__status") {{label}} }.into_view())}
                            }
                        })
                    }
                }
            }}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use zgui::prelude::*;
    use zgui::reactive::{LocalStorage, Mounted, install};

    use super::Thumbnail;
    use crate::service::file_inspection::{Kind, Source};
    use crate::ui::file_inspection::{Cache, Options};

    #[test]
    fn thumbnail_component_mounts_cleanly_for_fallback_and_uncached_image() {
        install().ok();
        let scope = Mounted::new();
        scope.with(|| {
            let cache = Cache::new(Options {
                capacity: 0,
                workers: 0,
                ..Options::default()
            });
            let src = Signal::<Source, LocalStorage>::stored_local(Source {
                path: PathBuf::from("/tmp/example.png"),
                kind: Kind::Image,
                size: Some(64),
                modified: None,
            });
            let _thumb = Thumbnail(cache, src, true, Classes::new());
        });
        scope.unmount();
    }
}
