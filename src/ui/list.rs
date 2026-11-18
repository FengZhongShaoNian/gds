//! Uniform list surface, alternating rows, blank-space hit geometry, and virtualization.

#[cfg(test)]
mod tests;

use zgui::geom::{Css, CssPx, Point, Rect};
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, css, view};

pub use crate::appearance::tokens::DataListStyle;
pub use zgui_ui::virtualize::{VirtualList, VirtualListProps, VirtualWindow, Virtualize};

const LIST_SHEET: &str = "gds-ui-list";

const LIST_CSS: &str = css!(
    ".gds-list-surface {
        display: flex;
        flex-direction: column;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow: hidden;
    }
    .gds-list-surface__scroll {
        display: block;
        flex: 1 1 0%;
        width: 100%;
        min-width: 0px;
        min-height: 0px;
        overflow-y: auto;
        overflow-x: hidden;
    }
    .gds-list-surface__items {
        display: flex;
        flex-direction: column;
        width: 100%;
        min-width: 0px;
    }
    .gds-list-row {
        display: flex;
        flex-direction: row;
        align-items: center;
        width: 100%;
        min-width: 0px;
        min-height: var(--gds-list-row-height, 40px);
        padding: var(--gds-list-cell-padding-y, 4px) var(--gds-list-cell-padding-x, 12px);
        border-radius: var(--gds-list-row-radius, 6px);
        color: var(--gds-foreground);
        font-size: var(--gds-list-name-size, 13px);
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
        transition:
            background-color var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1)),
            color var(--gds-motion-fast, 100ms) var(--gds-motion-ease-out, cubic-bezier(0, 0, 0.2, 1));
    }
    .gds-list-surface[data-style=\"striped\"] .gds-list-row[data-odd=\"true\"],
    .gds-list-surface[data-style=\"striped\"] .gds-list-row:not([data-index]):nth-child(even) {
        background-color: color-mix(in oklab, var(--gds-foreground) 3%, transparent);
    }
    .gds-list-row:hover {
        background-color: var(--gds-hover);
    }
    .gds-list-row[data-selected=\"true\"],
    .gds-list-row[data-selected=\"true\"]:hover {
        background-color: var(--gds-selection);
        color: var(--gds-selection-foreground);
    }
    .gds-list-row:focus-visible {
        outline: 1px solid var(--gds-primary);
        outline-offset: -1px;
    }"
);

fn install_list_stylesheets() {
    crate::ui::styles::install_ui_stylesheets();
    zgui::view::install_stylesheet(LIST_SHEET, LIST_CSS);
}

/// Visual density/alternating style for [`ListSurface`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListStyle {
    #[default]
    Striped,
    Plain,
}

impl ListStyle {
    const fn attr_value(self) -> &'static str {
        match self {
            Self::Striped => "striped",
            Self::Plain => "plain",
        }
    }
}

/// Geometry helper determining whether a press inside a [`ListSurface`] hit blank space
/// rather than the header, a populated row, or scrollbar lanes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListGeometry {
    pub rows: usize,
    pub row_height: f32,
    pub header_height: f32,
    pub inset_x: f32,
    pub bottom_inset: f32,
    pub scrollbar_width: f32,
}

impl Default for ListGeometry {
    fn default() -> Self {
        Self {
            rows: 0,
            row_height: DataListStyle::ROW_HEIGHT,
            header_height: DataListStyle::HEADER_HEIGHT,
            inset_x: 0.0,
            bottom_inset: DataListStyle::SCROLLBAR_LANE,
            scrollbar_width: DataListStyle::SCROLLBAR_LANE,
        }
    }
}

impl ListGeometry {
    #[must_use]
    pub fn is_blank(
        self,
        bounds: Rect<CssPx, Css>,
        position: Point<CssPx, Css>,
        scroll_y: CssPx,
    ) -> bool {
        if position.x.0 < bounds.origin.x.0
            || position.y.0 < bounds.origin.y.0
            || position.x.0 >= bounds.origin.x.0 + bounds.size.width.0
            || position.y.0 >= bounds.origin.y.0 + bounds.size.height.0
        {
            return false;
        }
        let local_x = position.x.0 - bounds.origin.x.0;
        let local_y = position.y.0 - bounds.origin.y.0;
        if local_y < self.header_height || local_y >= bounds.size.height.0 - self.bottom_inset {
            return false;
        }
        let right = bounds.size.width.0 - self.inset_x;
        if local_x >= right - self.scrollbar_width && local_x < right {
            return false;
        }
        local_x < self.inset_x
            || local_x >= right
            || local_y >= self.header_height + self.row_height * self.rows as f32 + scroll_y.0
    }
}

fn render_list_surface(
    style: ListStyle,
    node_ref: Option<NodeRef>,
    class: Classes,
    attrs: Attrs,
    content: AnyView,
) -> impl IntoView {
    install_list_stylesheets();
    view! {
        column(
            class = "gds-list-surface",
            node_ref = node_ref.unwrap_or_default(),
            attr:data-style = style.attr_value(),
            class = class,
            {..attrs}
        ) {
            {content}
        }
    }
}

/// Uniform list container applying GDS density styling.
#[component]
pub fn ListSurface(
    #[prop(default = ListStyle::Striped)] style: ListStyle,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    render_list_surface(style, node_ref, class, attrs, children.into_view_once())
}

/// Standard data-list row carrying alternating stripe and selection attributes.
#[component]
pub fn ListRow(
    #[prop(optional)] index: Option<usize>,
    #[prop(into, default = Signal::stored_local(false))] selected: Signal<bool, LocalStorage>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    children: Children,
) -> impl IntoView {
    install_list_stylesheets();
    let mut own = Attrs::new()
        .attribute(
            zgui::view::AttrName::new("data-index"),
            index.map(|i| i.to_string()),
        )
        .attribute(
            zgui::view::AttrName::new("data-odd"),
            index.is_some_and(|i| i % 2 == 1).then(|| "true".to_owned()),
        )
        .attribute(zgui::view::AttrName::new("data-selected"), move || {
            selected.get().then(|| "true".to_owned())
        })
        .a11y_from(A11yBinding::new(Role::ListItem).selected(move || selected.get()));
    if let Some(i) = index {
        own = own.a11y_from(A11yBinding::unspecified().row_index(i));
    }

    view! {
        row(
            class = "gds-list-row",
            node_ref = node_ref.unwrap_or_default(),
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}

/// Virtualized [`ListSurface`] building only rows visible inside its scrollport plus `overscan`.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn VirtualizedListSurface<V, F>(
    #[prop(into)] count: Signal<usize, LocalStorage>,
    #[prop(into, default = Signal::stored_local(DataListStyle::ROW_HEIGHT))] row_size: Signal<
        f32,
        LocalStorage,
    >,
    #[prop(default = 4)] overscan: usize,
    #[prop(default = ListStyle::Striped)] list_style: ListStyle,
    #[prop(into, optional)] label: Option<String>,
    #[prop(optional)] node_ref: Option<NodeRef>,
    #[prop(into, optional)] class: Classes,
    #[prop(attrs)] attrs: Attrs,
    row: F,
) -> impl IntoView
where
    V: IntoView + 'static,
    F: Fn(usize) -> V + 'static,
{
    install_list_stylesheets();
    let viewport = node_ref.unwrap_or_default();
    let seen = Virtualize::new(viewport, count, row_size, overscan);
    let window = Signal::derive_local(move || seen.window());
    let row = std::rc::Rc::new(row);

    let mut semantics = A11yBinding::new(Role::List);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let scroll_view = AnyView::new(view! {
        scroll(
            node_ref = viewport,
            class = "gds-list-surface__scroll",
            {..Attrs::new().a11y_from(semantics)}
        ) {
            column(
                class = "gds-list-surface__items",
                style:padding-top = move || Some(format!("{}px", window.get().lead)),
                style:padding-bottom = move || Some(format!("{}px", window.get().trail))
            ) {
                for index in move || window.get().indices(), key = |index: &usize| *index {
                    {row(index)}
                }
            }
        }
    });

    render_list_surface(list_style, None, class, attrs, scroll_view)
}
