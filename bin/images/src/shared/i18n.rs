//! Application localization catalogs backed by GDS Fluent runtime.

#[cfg(test)]
mod tests;

use gds::app::AppExt as _;
#[cfg(test)]
pub use gds::i18n::{Catalog, FluentArgs};
pub use gds::i18n::{Error, Language, text};

/// Installs `GDS Images` Fluent resources for the detected desktop language.
///
/// # Errors
/// Returns [`Error`] if Fluent resource parsing fails or contains conflicting keys.
pub fn install(cx: &mut gpui_kit::App) -> Result<(), Error> {
    let language = Language::detect();
    let source = resource_for(language);
    cx.gds_mut().i18n().install_resources(language, &[source])
}

pub(crate) fn resource_for(language: Language) -> &'static str {
    match language {
        Language::English => include_str!("i18n/assets/en-US/app.ftl"),
        Language::Chinese => include_str!("i18n/assets/zh-CN/app.ftl"),
    }
}
