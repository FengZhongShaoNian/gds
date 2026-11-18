use std::time::SystemTime;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use super::{Format, Presentation, format, now, tick_signal};
use crate::app::Gds;

/// Timestamp builder usable directly inside `view!` expressions or via [`TimestampView`].
#[derive(Clone, Copy, Debug)]
pub struct Timestamp {
    value: Option<SystemTime>,
    override_format: Option<Format>,
}

impl Timestamp {
    #[must_use]
    pub fn new(value: Option<SystemTime>) -> Self {
        Self {
            value,
            override_format: None,
        }
    }

    #[must_use]
    pub fn format(mut self, format: Format) -> Self {
        self.override_format = Some(format);
        self
    }

    #[must_use]
    pub fn render_text(self) -> String {
        let fmt = self.override_format.unwrap_or_else(format);
        format_presentation(fmt.presentation(self.value, now()))
    }
}

impl IntoView for Timestamp {
    type Output = AnyView;

    fn into_view(self) -> Self::Output {
        AnyView::new(view! {
            TimestampView(value = self.value, override_format = self.override_format)
        })
    }
}

/// Reactive timestamp component rendering localized relative or absolute dates.
#[component]
pub fn TimestampView(
    #[prop(into)] value: Signal<Option<SystemTime>, LocalStorage>,
    #[prop(optional)] override_format: Option<Format>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
) -> impl IntoView {
    let ticks = tick_signal();
    view! {
        text(class = "gds-timestamp", class = class, {..attrs}) {{move || {
            let _ = ticks.get();
            let fmt = override_format.unwrap_or_else(format);
            format_presentation(fmt.presentation(value.get(), now()))
        }}}
    }
}

fn format_presentation(presentation: Presentation) -> String {
    let gds = Gds::try_current();
    match presentation {
        Presentation::Missing => "—".to_owned(),
        Presentation::Epoch => gds
            .as_ref()
            .map_or_else(|| "Epoch".to_owned(), |g| g.i18n().text("time-epoch")),
        Presentation::JustNow => gds
            .as_ref()
            .map_or_else(|| "Just now".to_owned(), |g| g.i18n().text("time-just-now")),
        Presentation::Minutes(n) => gds.as_ref().map_or_else(
            || format!("{n}m ago"),
            |g| g.i18n().count("time-minutes-ago", n as usize),
        ),
        Presentation::Today { time } => time,
        Presentation::Yesterday { time } => format!("Yesterday {time}"),
        Presentation::Days(n) => format!("{n}d ago"),
        Presentation::Weeks(n) => format!("{n}w ago"),
        Presentation::Months(n) => format!("{n}mo ago"),
        Presentation::Years(n) => format!("{n}y ago"),
        Presentation::Absolute { text } => text,
    }
}
