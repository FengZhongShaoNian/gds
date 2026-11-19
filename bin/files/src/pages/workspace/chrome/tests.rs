use std::{
    cell::RefCell, collections::HashMap, collections::HashSet, path::Path, rc::Rc, sync::Arc,
};

use gds::{
    app::Gds,
    appearance::tokens::{COLLAPSED_SIDEBAR_WIDTH, SIDEBAR_WIDTH},
    i18n::Language,
    service::task_manager::Scope,
    ui::sidebar::SidebarPresentation,
};
use zgui::{
    app::Fonts,
    atlas::{SinkError, TextureFormat, TextureId, TextureSink},
    geom::{Device, DevicePx, Matrix4, Point, Rect, Size, transformed_bounds},
    platform::{AppHandler, PlatformError, Surface, SurfaceEvent},
    prelude::*,
    reactive::{Mounted, flush, install},
    render::{
        ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
        Renderer, TextureHandle,
    },
    runtime::AppError,
    view,
    vocab::Role,
};
use zgui_platform_headless::{Harness, OffscreenSurface};

use super::{
    Breadcrumb, TabCycleTargets, compute_tab_cycle_targets, default_volumes_bottom_height,
};
use crate::{
    entities::{
        location::{Location, places::Places, scheme::TagAddress, volumes},
        preferences,
    },
    pages::workspace::ui::{Browser, LaunchOptions},
    shared::i18n,
};

struct NoopSink;

impl TextureSink for NoopSink {
    fn create_texture(
        &mut self,
        _: TextureId,
        _: Size<i32, Device>,
        _: TextureFormat,
    ) -> Result<(), SinkError> {
        Ok(())
    }

    fn write_texture(
        &mut self,
        _: TextureId,
        _: Rect<i32, Device>,
        _: TextureFormat,
        _: &[u8],
    ) -> Result<(), SinkError> {
        Ok(())
    }

    fn destroy_texture(&mut self, _: TextureId) {}
}

thread_local! {
    static TAB_NODES: RefCell<HashMap<u64, (NodeRef, NodeRef)>> = RefCell::new(HashMap::new());
    static LAST_VECTOR_INKS: RefCell<Vec<Rect<DevicePx, Device>>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn record_tab_nodes(id: u64, tab: NodeRef, corner: NodeRef) {
    TAB_NODES.with(|slot| {
        slot.borrow_mut().insert(id, (tab, corner));
    });
}

fn clear_recorded_tab_nodes() {
    TAB_NODES.with(|slot| slot.borrow_mut().clear());
    LAST_VECTOR_INKS.with(|slot| slot.borrow_mut().clear());
}

fn recorded_tab_nodes(id: u64) -> Option<(NodeRef, NodeRef)> {
    TAB_NODES.with(|slot| slot.borrow().get(&id).copied())
}

fn last_vector_inks() -> Vec<Rect<DevicePx, Device>> {
    LAST_VECTOR_INKS.with(|slot| slot.borrow().clone())
}

struct NoopRenderer {
    target: Option<RenderTarget>,
    sink: NoopSink,
}

impl Renderer for NoopRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, scene: &zgui::scene::Scene, _: &zgui::bits::DamageSet) -> FrameOutcome {
        if !scene.primitives.mono_sprites.is_empty() {
            let placements = scene.placements();
            let matrices: Vec<Matrix4> = placements.matrices().collect();
            LAST_VECTOR_INKS.with(|slot| {
                let mut inks = slot.borrow_mut();
                inks.clear();
                for sprite in &scene.primitives.mono_sprites {
                    let [x, y, w, h] = sprite.bounds;
                    if w <= 0.0 || h <= 0.0 {
                        continue;
                    }
                    let local = Rect::new(
                        Point::new(DevicePx(x), DevicePx(y)),
                        Size::new(DevicePx(w), DevicePx(h)),
                    );
                    let placed = match matrices.get(sprite.transform as usize) {
                        Some(matrix) => transformed_bounds(matrix, local),
                        None => local,
                    };
                    inks.push(placed);
                }
            });
        }
        FrameOutcome::Presented(FrameStats::default())
    }

    fn register_external(&mut self, _: ExternalTexture) -> TextureHandle {
        TextureHandle(1)
    }

    fn release_external(&mut self, _: TextureHandle) {}

    fn memory(&self) -> MemoryReport {
        MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        &mut self.sink
    }
}

fn noop_renderer(
    _: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    Ok(Box::new(NoopRenderer {
        target: Some(target),
        sink: NoopSink,
    }))
}

#[derive(Clone, Debug)]
struct LiveButton {
    label: String,
    x0: f64,
    x1: f64,
}

impl LiveButton {
    fn center_x(&self) -> f64 {
        (self.x0 + self.x1) * 0.5
    }
}

fn compose_affine(outer: [f64; 6], inner: [f64; 6]) -> [f64; 6] {
    [
        outer[0] * inner[0] + outer[2] * inner[1],
        outer[1] * inner[0] + outer[3] * inner[1],
        outer[0] * inner[2] + outer[2] * inner[3],
        outer[1] * inner[2] + outer[3] * inner[3],
        outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
        outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
    ]
}

fn transform_x_span(m: [f64; 6], r: (f64, f64, f64, f64)) -> (f64, f64) {
    let xs = [
        m[0] * r.0 + m[2] * r.1 + m[4],
        m[0] * r.2 + m[2] * r.1 + m[4],
        m[0] * r.2 + m[2] * r.3 + m[4],
        m[0] * r.0 + m[2] * r.3 + m[4],
    ];
    let min_x = xs.into_iter().fold(f64::INFINITY, f64::min);
    let max_x = xs.into_iter().fold(f64::NEG_INFINITY, f64::max);
    (min_x, max_x)
}

fn collect_live_buttons(surface: &OffscreenSurface) -> Vec<LiveButton> {
    let mut nodes = HashMap::new();
    let mut root = None;
    for update in surface.a11y_log() {
        if let Some(tree) = update.tree {
            root = Some(tree.root);
        }
        for (id, node) in update.nodes {
            nodes.insert(id, node);
        }
    }
    let Some(root_id) = root else {
        return Vec::new();
    };
    let mut stack = vec![(root_id, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0])];
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    while let Some((id, world)) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(node) = nodes.get(&id) else {
            continue;
        };
        let next_world = match node.transform() {
            Some(t) => compose_affine(world, t.as_coeffs()),
            None => world,
        };
        if node.role() == Role::Button
            && let Some(rect) = node.bounds()
        {
            let (x0, x1) = transform_x_span(next_world, (rect.x0, rect.y0, rect.x1, rect.y1));
            out.push(LiveButton {
                label: node.label().unwrap_or_default().to_owned(),
                x0,
                x1,
            });
        }
        for &child in node.children().iter().rev() {
            stack.push((child, next_world));
        }
    }
    out
}

fn find_button_by_label<'a>(buttons: &'a [LiveButton], label: &str) -> Option<&'a LiveButton> {
    buttons.iter().find(|btn| btn.label == label)
}

fn run_chrome_harness(dir: &Path, check: impl FnOnce(&mut Harness<Box<dyn AppHandler>>, &Browser)) {
    let start = dir.to_path_buf();
    let browser_slot: Rc<RefCell<Option<Browser>>> = Rc::new(RefCell::new(None));
    let slot_for_view = Rc::clone(&browser_slot);
    let check_slot = RefCell::new(Some(check));

    let driver = move |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
        let mut harness = Harness::new(handler);
        if let Some(offscreen) = harness.platform().offscreens().first() {
            offscreen.set_a11y_listener(true);
        }
        harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
            DevicePx(1280.0),
            DevicePx(800.0),
        )));
        harness.settle(16);
        let browser = browser_slot.borrow().clone().expect("browser constructed");
        let run_check = check_slot.borrow_mut().take().expect("check closure");
        run_check(&mut harness, &browser);
        harness.shut_down();
        Ok(())
    };

    zgui::app()
        .with_fonts(Fonts::shipped_only())
        .with_renderer(Box::new(noop_renderer))
        .with_size(1280.0, 800.0)
        .run_on(driver, move || {
            let scope = Scope::default();
            let app = Gds::install(scope.clone());
            app.appearance().motion().set_enabled(false);
            let _ = app.appearance().install_window_sheet();
            app.i18n()
                .install_catalog(i18n::prepare(Language::English).expect("English catalog"));
            let _prefs = preferences::Service::install(preferences::Preferences::default(), &scope)
                .expect("preferences service");
            let browser = Browser::new(
                LaunchOptions {
                    places: Ok(Places::default()),
                    start: start.clone(),
                    home: start.clone(),
                    picker: None,
                },
                volumes::Service::new(scope),
                Rc::new(|_| {}),
            );
            *slot_for_view.borrow_mut() = Some(browser.clone());
            view! {
                column(style:width = "100%", style:height = "100%") {
                    row(style:width = "100%", style:height = "38px") {
                        {browser.toolbar_controls()}
                    }
                    row(style:flex = "1 1 0%", style:width = "100%", style:min-height = "0px") {
                        {browser.sidebar(SidebarPresentation::default())}
                        {browser.folder_filter()}
                    }
                }
            }
        })
        .expect("headless chrome harness");
}

#[test]
fn filesystem_breadcrumbs_ignore_curdir_and_mark_only_leaf_as_current() {
    let crumbs = Breadcrumb::for_location(
        &Location::from(Path::new("/mnt/./storage/projects")),
        Some(Path::new("/mnt/./storage/projects")),
    );
    assert_eq!(crumbs.len(), 4);
    assert_eq!(crumbs[0].label, "/");
    assert!(!crumbs[0].current);
    assert!(!crumbs[0].separated);
    assert_eq!(crumbs[1].label, "mnt/");
    assert_eq!(crumbs[1].location, Location::from(Path::new("/mnt")));
    assert_eq!(crumbs[2].label, "storage/");
    assert_eq!(crumbs[3].label, "projects");
    assert!(crumbs[3].current);
}

#[test]
fn tag_breadcrumbs_build_clickable_separated_hierarchy_from_root_to_leaf() {
    let _ = install();
    let root = Mounted::new();
    root.with(|| {
        let app = Gds::install(Scope::default());
        app.i18n()
            .install_catalog(i18n::prepare(Language::English).expect("English catalog"));

        let leaf = TagAddress::new("work/urgent").expect("valid tag path");
        let crumbs = Breadcrumb::for_location(&Location::Tags(leaf.clone()), None);
        let root_tag = TagAddress::new("").expect("root tag");
        let work_tag = TagAddress::new("work").expect("work tag");

        assert_eq!(crumbs.len(), 3);
        assert_eq!(crumbs[0].location, Location::Tags(root_tag));
        assert!(!crumbs[0].current);
        assert!(!crumbs[0].separated);

        assert_eq!(crumbs[1].label, "work");
        assert_eq!(crumbs[1].location, Location::Tags(work_tag));
        assert!(!crumbs[1].current);
        assert!(crumbs[1].separated);

        assert_eq!(crumbs[2].label, "urgent");
        assert_eq!(crumbs[2].location, Location::Tags(leaf));
        assert!(crumbs[2].current);
        assert!(crumbs[2].separated);
    });
    root.unmount();
}

#[test]
fn tab_cycle_targets_wrap_around_strip_and_stay_empty_for_single_tab() {
    assert_eq!(
        compute_tab_cycle_targets(&[7], 7),
        TabCycleTargets::default()
    );
    assert_eq!(
        compute_tab_cycle_targets(&[10, 20, 30], 10),
        TabCycleTargets {
            previous: Some(30),
            next: Some(20),
        }
    );
    assert_eq!(
        compute_tab_cycle_targets(&[10, 20], 10),
        TabCycleTargets {
            previous: Some(20),
            next: Some(20),
        }
    );
}

#[test]
fn volumes_default_bottom_height_caps_visible_and_mounted_rows_at_four() {
    let single_unmounted = default_volumes_bottom_height(1, 0);
    let single_mounted = default_volumes_bottom_height(1, 1);
    let four_mounted = default_volumes_bottom_height(4, 4);
    let eight_mounted = default_volumes_bottom_height(8, 8);

    assert!(single_mounted > single_unmounted);
    assert_eq!(four_mounted, eight_mounted);
}

#[test]
fn responsive_toolbar_and_collapsed_rail_center_actions_on_29px_axis() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let _guard = runtime.enter();
    let temp = tempfile::tempdir().expect("tempdir");

    let pending_label = temp.path().display().to_string();

    run_chrome_harness(temp.path(), |harness, browser| {
        let surface = harness
            .platform()
            .offscreens()
            .first()
            .cloned()
            .expect("offscreen surface");

        let desktop_buttons = collect_live_buttons(&surface);
        let toggle_lbl = gds::i18n::text_untracked("sidebar-toggle");
        let back_lbl = gds::i18n::text_untracked("back");
        let forward_lbl = gds::i18n::text_untracked("forward");
        let parent_lbl = gds::i18n::text_untracked("parent");
        let new_tab_lbl = gds::i18n::text_untracked("new-tab");

        assert!(find_button_by_label(&desktop_buttons, &pending_label).is_some());
        assert!(find_button_by_label(&desktop_buttons, &toggle_lbl).is_some());
        assert!(find_button_by_label(&desktop_buttons, &back_lbl).is_some());
        assert!(find_button_by_label(&desktop_buttons, &forward_lbl).is_some());
        assert!(find_button_by_label(&desktop_buttons, &parent_lbl).is_some());
        assert_eq!(browser.sidebar_width().get_untracked(), SIDEBAR_WIDTH);

        browser.update(|ws| ws.tab_mut().navigation.cancel_pending());
        browser.0.window_width.set(960.0);
        flush();
        harness.settle(16);

        let compact_buttons = collect_live_buttons(&surface);
        assert!(find_button_by_label(&compact_buttons, &toggle_lbl).is_none());
        assert!(find_button_by_label(&compact_buttons, &back_lbl).is_none());
        assert!(find_button_by_label(&compact_buttons, &forward_lbl).is_none());
        assert!(find_button_by_label(&compact_buttons, &parent_lbl).is_none());
        assert!(find_button_by_label(&compact_buttons, "/").is_some());
        assert_eq!(
            browser.sidebar_width().get_untracked(),
            COLLAPSED_SIDEBAR_WIDTH
        );

        let new_tab_btn = find_button_by_label(&compact_buttons, &new_tab_lbl)
            .expect("new-tab button in collapsed rail");
        let locations_lbl = gds::i18n::text_untracked("locations");
        let locations_btn = find_button_by_label(&compact_buttons, &locations_lbl)
            .expect("locations heading button in collapsed rail");
        let rail_center = f64::from(COLLAPSED_SIDEBAR_WIDTH) * 0.5;
        assert!(
            (new_tab_btn.center_x() - rail_center).abs() <= 1.0,
            "expected collapsed + button centered at x={rail_center}, got {:.2} ({:.1}..{:.1})",
            new_tab_btn.center_x(),
            new_tab_btn.x0,
            new_tab_btn.x1
        );
        assert!(
            (locations_btn.center_x() - rail_center).abs() <= 1.0
                && locations_btn.x1 <= f64::from(COLLAPSED_SIDEBAR_WIDTH),
            "expected collapsed locations heading centered at x={rail_center}, got {:.2} ({:.1}..{:.1})",
            locations_btn.center_x(),
            locations_btn.x0,
            locations_btn.x1
        );
    });
}

#[test]
fn collapsed_pinned_tab_renders_single_corner_pin_button_without_pinned_icon_badge() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let _guard = runtime.enter();
    let temp = tempfile::tempdir().expect("tempdir");
    let sub = temp.path().join("workspace_dir");
    std::fs::create_dir_all(&sub).expect("create workspace_dir");
    clear_recorded_tab_nodes();

    run_chrome_harness(&sub, |harness, browser| {
        let surface = harness
            .platform()
            .offscreens()
            .first()
            .cloned()
            .expect("offscreen surface");

        browser.update(|ws| ws.tab_mut().navigation.cancel_pending());
        browser.0.window_width.set(960.0);
        flush();
        harness.settle(16);

        let tab_id = browser.read_untracked(|ws| ws.tab().id);
        let (unpinned_tab_ref, _) =
            recorded_tab_nodes(tab_id).expect("recorded unpinned tab nodes");
        let unpinned_tab_bounds = unpinned_tab_ref
            .window_bounds()
            .expect("unpinned tab bounds");
        let unpinned_vectors: Vec<_> = last_vector_inks()
            .into_iter()
            .filter(|ink| {
                ink.origin.y.0 >= unpinned_tab_bounds.origin.y.0 - 2.0
                    && (ink.origin.y.0 + ink.size.height.0)
                        <= (unpinned_tab_bounds.origin.y.0
                            + unpinned_tab_bounds.size.height.0
                            + 2.0)
            })
            .collect();
        assert!(
            !unpinned_vectors.is_empty(),
            "expected folder icon vectors on collapsed unpinned tab"
        );

        // Pin the tab while sidebar is collapsed (58px rail).
        browser.toggle_pin(0);
        flush();
        harness.settle(16);

        let (pinned_tab_ref, corner_ref) =
            recorded_tab_nodes(tab_id).expect("recorded pinned tab nodes");
        let tab_bounds = pinned_tab_ref.window_bounds().expect("pinned tab bounds");
        let corner_bounds = corner_ref
            .window_bounds()
            .expect("corner pin button bounds");

        let unpin_lbl = gds::i18n::text_untracked("unpin-tab");
        let unpin_buttons: Vec<_> = collect_live_buttons(&surface)
            .into_iter()
            .filter(|btn| btn.label == unpin_lbl)
            .collect();
        assert_eq!(
            unpin_buttons.len(),
            1,
            "collapsed pinned tab must expose only the single CornerButton unpin action, got {unpin_buttons:?}"
        );
        let rail_center = f64::from(COLLAPSED_SIDEBAR_WIDTH) * 0.5;
        assert!(
            unpin_buttons[0].center_x() > rail_center,
            "expected CornerButton pin icon in top-right of collapsed rail (> {rail_center}), got {:?}",
            unpin_buttons[0]
        );

        let pinned_vectors: Vec<_> = last_vector_inks()
            .into_iter()
            .filter(|ink| {
                ink.origin.y.0 >= tab_bounds.origin.y.0 - 4.0
                    && (ink.origin.y.0 + ink.size.height.0)
                        <= (tab_bounds.origin.y.0 + tab_bounds.size.height.0 + 4.0)
            })
            .collect();

        // Each visible `<vector>` Icon rasterizes to one `ColorSprite`:
        // - Unpinned collapsed tab: 1 icon sprite (centered 18x18 folder icon).
        // - Pinned collapsed tab: 2 icon sprites (centered 18x18 folder icon + single 11x11
        //   `CornerButton` Pin icon inside `corner_bounds`), NEVER 3 from a duplicate `PinnedIcon`!
        assert_eq!(
            unpinned_vectors.len(),
            1,
            "expected exactly 1 icon sprite on collapsed unpinned tab, got {unpinned_vectors:?}"
        );
        assert_eq!(
            pinned_vectors.len(),
            2,
            "expected exactly 2 icon sprites on collapsed pinned tab (folder + single CornerButton pin, never duplicate PinnedIcon), got {pinned_vectors:?}"
        );
        let has_top_left_pin_badge = pinned_vectors.iter().any(|ink| {
            let cx = ink.origin.x.0 + ink.size.width.0 * 0.5;
            let in_corner_btn = ink.origin.x.0 >= corner_bounds.origin.x.0 - 2.0
                && (ink.origin.x.0 + ink.size.width.0)
                    <= corner_bounds.origin.x.0 + corner_bounds.size.width.0 + 2.0;
            !in_corner_btn && f64::from(cx) < rail_center - 4.0
        });
        assert!(
            !has_top_left_pin_badge,
            "collapsed pinned tab must not render a top-left PinnedIcon badge; vectors={pinned_vectors:?}"
        );
    });
}
