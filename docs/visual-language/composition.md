# Component Composition

Reusable GDS structural components expose anatomical sub-elements instantiated with `Default::default()` in `gds-uim` (`el!`), reserving `new(...)` for required data or identity parameters. Callers override styles via attributes and pass content as ordinary children.

## `ContentPage` (`gds::ui::page::*`)

Standard workspace page skeleton (`24 px` horizontal / `16 px` vertical header padding, scaled `22 px` semibold title, `18 px` table inset inside `ListSurface` so side margins clear selection on click):

```rust,ignore
el! {
    .ContentPage::default { # "directory-page" } [
        .PageHeader::default [
            .PageTitle::default [title],
            .PageActions::default [..actions],
        ],
        @if (let Some(notice) = notice)
            .PageNotice::default [notice],
        @end,
        .PageContent::default [body],
    ]
}
```

Wrap data tables in `PageTableArea::default().id(id).child(table)` to preserve blank-area deselection and context menus. While loading a directory or remote target, show GPUI Kit's `Spinner` with the target address and a cancel action without presenting stale entries under the new heading.

## `SettingRow` (`gds::ui::settings::*`)

```rust,ignore
el! {
    .SettingRow::default [
        .SettingLabel::default [
            title,
            .SettingDescription::default [description],
        ],
        .SettingActions::default [
            .SettingReset::default [..reset],
            .SettingControl::default [control],
        ],
    ]
}
```

`SettingReset` reserves fixed horizontal space even when empty so controls never shift when a field diverges from `Preferences::default()`.

## `DragCard` (`gds::ui::drag::*`)

Compact multi-item drag preview anchored below-right of the cursor (single-item drags reuse the item's native icon/thumbnail and label directly):

```rust,ignore
el! {
    .DragCard::default { offset: cursor_offset } [
        .DragCardHeader::default [icon, title],
        .DragCardContent::default [..items],
        @if (remaining > 0)
            .DragCardFooter::default [more_label],
        @end,
    ]
}
```

Displays total item count in the header, up to three category-icon + name previews, and an overflow count in the footer.

## Navigation, Frames, Splits & Dialogs

- **`NavItem` (`gds::ui::navigation::*`)**: Composes `NavItemContent`, `NavItemHeader`, `NavItemLabel`, `NavItemDetails`, and `NavItemActions` inside a `38 px` target (`18 px` leading icon, `4 px` row spacing, optional pin and hover-revealed close slot). Agents also provides a passive `ContentRow` (`bin/agents/src/shared/ui/content_row.rs`) for audio and activity rows.
- **`Frame` (`gds::ui::Frame`)**: `Frame::default().sidebar_width(width)` aligns toolbar, workspace, and footer column boundaries; sidebar-free views use `Frame::without_sidebar()`.
- **`VerticalSplit` (`gds::ui::split::VerticalSplit`)**: `VerticalSplit::new(state).top(...).bottom(...)` wraps GPUI Kit's `resize_handle` with height clamping, keyboard stepping, and saved-preference persistence on release.
- **`DialogActions` (`gds::ui::DialogActions`)**: Standard modal footer (`DialogActions::default().primary_label(...).primary_icon(IconName::Check).cancel_label(...)`, plus `.danger()` for destructive confirmations). Emits Kit's Confirm/Cancel commands while leaving validation and close callbacks to the host dialog.
- **Direct GPUI Kit primitives**: Reuse Kit's `Empty`, `Message`, `Attachment`, `Clipboard` (copying raw un-truncated text), `Kbd`, and `Collapsible` (`@if (expanded) content: $(render_body(cx)); @end` to defer heavy Markdown/tool-output construction until expanded).

## Status Popovers & Remote `DirectoryPicker`

- **Files footer status popovers**: Background tasks, selection, and clipboard summary controls sit after the item count (`32 × 32 px` icon + badge in compact mode; icon + summary text in expanded mode, transitioning over `160 ms`). Clicking opens an upward `ListItem` popover where row-tail actions cancel a task, deselect an item, or remove an entry from the file clipboard.
- **`DirectoryPicker` (`gds::ui::picker::directory::DirectoryPicker`)**: Composes `Picker`, `PickerToolbar`, `PickerLocation`, `PickerContent`, and `PickerStatus` inside a host alert dialog for paginated remote directory browsing (`Loader`), supporting `Ctrl+L`, `Ctrl+F`, `Ctrl+H`, `Ctrl+R`, and `Alt+Left/Right/Up` without any BAS or local-FS coupling.

## About (`gds::ui::about::About`)

Shared application identity view mounted inside an alert dialog in Files (`WindowExt::open_alert_dialog`) and as a page/window in Agents:

```rust,ignore
use gds::ui::about::{About, Application};

let about = cx.new(|cx| About::new(Application::cargo(env!("CARGO_BIN_NAME")), cx));
```

- **Metadata registration**: Reads `package.{version,homepage,authors,license}` and `[package.metadata.gds.about]` (feedback URL, root `LICENSE`, shared credits, and per-binary `applications.<bin-name>` `name` + `icon`). `build.rs` embeds the 9-character Git commit hash as `GDS_BUILD_TAG` (falling back to flake revision or explicit `GDS_BUILD_TAG` env var in `.git`-less sandboxes).
- **Behavior**: Displays `assets/<app>/icon-solid.svg` and the product name, separate copyable version and build-tag rows, external Website/Feedback links, Credits, and the embedded Apache-2.0 license with `Esc` / `Alt+Left` sub-page navigation.
