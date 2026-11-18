use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use zgui::app::app;
use zgui::geom::{CssPx, Point};
use zgui::platform::{AppHandler, PlatformError, Surface, SurfaceEvent};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Signal, UnsyncCallback};
use zgui::view;
use zgui::view::focused_node;
use zgui::vocab::{
    Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction,
    PointerButton, PointerEvent, Timestamp,
};
use zgui_platform_headless::Harness;

use super::{BLINK_INTERVAL, TerminalViewProps};
use crate::app::Gds;
use crate::service::task_manager::Supervisor;
use crate::service::terminal::{Session, SpawnOptions};
use crate::ui::drag::DragOverlayState;
use crate::ui::terminal::{Preferences, Terminal};

#[derive(Default)]
struct NullRenderer {
    target: Option<zgui::render::RenderTarget>,
    atlas: zgui::atlas::MemorySink,
}

impl zgui::render::Renderer for NullRenderer {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        zgui::render::RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: zgui::render::RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<zgui::render::RenderTarget> {
        self.target
    }

    fn draw(
        &mut self,
        _scene: &zgui::scene::Scene,
        _damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        zgui::render::FrameOutcome::Presented(zgui::render::FrameStats {
            vector_passes: 0,
            draw_calls: 0,
            damage_px: 0,
            bytes_uploaded: 0,
            memory: zgui::render::MemoryReport::ZERO,
        })
    }

    fn register_external(
        &mut self,
        _texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        zgui::render::TextureHandle(0)
    }

    fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}

    fn memory(&self) -> zgui::render::MemoryReport {
        zgui::render::MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.atlas
    }
}

struct TestHostState {
    terminal: Terminal,
    outer_ref: NodeRef,
    next_ref: NodeRef,
    terminal_ref: NodeRef,
    focused: Signal<Option<NodeId>, LocalStorage>,
    drag: DragOverlayState<Vec<PathBuf>>,
    workspace_actions: Rc<Cell<usize>>,
    settings: Rc<Cell<usize>>,
    toggles: Rc<Cell<usize>>,
    exits: Rc<Cell<usize>>,
    gds: Gds,
}

fn run_headless_terminal(
    options: SpawnOptions,
    preferences: Preferences,
    run_assertions: impl FnOnce(&mut Harness<Box<dyn AppHandler>>, &TestHostState),
) {
    let supervisor = Supervisor::new(false).expect("test Tokio supervisor");
    let scope = supervisor.scope();
    let runtime = scope.runtime_handle().expect("supervisor runtime").clone();
    let state_slot: Rc<RefCell<Option<TestHostState>>> = Rc::new(RefCell::new(None));
    let slot_for_view = Rc::clone(&state_slot);
    let options_cell = RefCell::new(Some((options, preferences)));

    let handler = app()
        .with_title("terminal-test")
        .with_application_id("dev.gds.TerminalTest")
        .with_size(800.0, 480.0)
        .with_stylesheet(":root { display: flex; width: 100%; height: 100%; }")
        .with_renderer(Box::new(
            move |_surface: &std::sync::Arc<dyn Surface>, target: zgui::render::RenderTarget| {
                let mut renderer = NullRenderer::default();
                zgui::render::Renderer::configure(&mut renderer, target);
                Ok::<_, zgui::Error>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
            },
        ))
        .with_context(move || {
            let gds = Gds::install(scope.clone());
            gds.appearance()
                .theme()
                .apply(&crate::appearance::theme::Appearance {
                    source: crate::appearance::theme::Source::Builtin,
                    ..crate::appearance::theme::Appearance::default()
                })
                .expect("default test theme");
        })
        .into_handler(move || -> AnyView {
            crate::ui::styles::install_ui_stylesheets();
            let gds = Gds::current();
            let (opts, prefs) = options_cell
                .borrow_mut()
                .take()
                .expect("view factory called once");
            let terminal =
                Terminal::spawn(opts, prefs, &gds.tasks().scope()).expect("spawn test terminal");
            let outer_ref = NodeRef::new();
            let next_ref = NodeRef::new();
            let terminal_ref = NodeRef::new();
            let focused = focused_node();
            let drag = DragOverlayState::<Vec<PathBuf>>::new();
            let workspace_actions = Rc::new(Cell::new(0));
            let settings = Rc::new(Cell::new(0));
            let toggles = Rc::new(Cell::new(0));
            let exits = Rc::new(Cell::new(0));

            *slot_for_view.borrow_mut() = Some(TestHostState {
                terminal: terminal.clone(),
                outer_ref,
                next_ref,
                terminal_ref,
                focused,
                drag,
                workspace_actions: Rc::clone(&workspace_actions),
                settings: Rc::clone(&settings),
                toggles: Rc::clone(&toggles),
                exits: Rc::clone(&exits),
                gds,
            });

            let on_host_key = {
                let workspace_actions = Rc::clone(&workspace_actions);
                let settings = Rc::clone(&settings);
                let toggles = Rc::clone(&toggles);
                move |ev: &mut EventCx<'_, zgui::view::events::KeyDown>| {
                    let inside_terminal = terminal_ref
                        .get_untracked()
                        .is_some_and(|id| focused.get_untracked() == Some(id));
                    if !inside_terminal
                        && ev.modifiers == Modifiers::CONTROL
                        && matches!(&ev.key_without_modifiers, Key::Character(c) if c.eq_ignore_ascii_case("w"))
                    {
                        workspace_actions.set(workspace_actions.get() + 1);
                        ev.stop_propagation();
                        ev.prevent_default();
                    } else if ev.modifiers == Modifiers::CONTROL
                        && matches!(&ev.key_without_modifiers, Key::Character(ch) if ch.as_str() == "," || ch.as_str() == "`")
                    {
                        if let Key::Character(ch) = &ev.key_without_modifiers
                            && ch.as_str() == ","
                        {
                            settings.set(settings.get() + 1);
                        } else {
                            toggles.set(toggles.get() + 1);
                        }
                        ev.stop_propagation();
                        ev.prevent_default();
                    }
                }
            };

            let on_terminal_key = UnsyncCallback::new({
                let settings = Rc::clone(&settings);
                let toggles = Rc::clone(&toggles);
                let exits = Rc::clone(&exits);
                move |(key, modifiers): (Key, Modifiers)| {
                    if modifiers == Modifiers::CONTROL
                        && matches!(&key, Key::Character(c) if c == ",")
                    {
                        settings.set(settings.get() + 1);
                        true
                    } else if modifiers == Modifiers::CONTROL
                        && matches!(&key, Key::Character(c) if c == "`")
                    {
                        toggles.set(toggles.get() + 1);
                        true
                    } else if modifiers == Modifiers::NONE && key == Key::Named(NamedKey::F6) {
                        exits.set(exits.get() + 1);
                        true
                    } else {
                        false
                    }
                }
            });

            let term_for_drop = terminal.clone();
            AnyView::new(view! {
                box(
                    style = "display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0;",
                    on:key_down = on_host_key,
                ) {
                    box(
                        node_ref = outer_ref,
                        tabindex = {Focus::Sequential},
                        style = "width: 20px; height: 10px;",
                    )
                    box(
                        node_ref = next_ref,
                        tabindex = {Focus::Sequential},
                        style = "width: 20px; height: 10px;",
                    )
                    box(
                        style = "flex: 1 1 0; width: 100%; min-height: 0; display: flex;",
                        on:pointer_up = move |_| {
                            if let Some(paths) = drag.end() {
                                term_for_drop.paste_paths(&paths);
                                term_for_drop.focus();
                            }
                        },
                    ) {
                        TerminalView(
                            terminal = terminal,
                            on_key = on_terminal_key,
                            drag_active = drag.is_active(),
                            node_ref = terminal_ref,
                        )
                    }
                }
            })
        })
        .expect("build headless terminal test app");

    handler
        .drive(|app| -> Result<(), PlatformError> {
            let mut harness = Harness::new(app);
            pump_frames(&mut harness);
            harness.deliver_to_first(SurfaceEvent::Focused(true));
            pump_frames(&mut harness);
            let borrowed = state_slot.borrow();
            let host = borrowed.as_ref().expect("host test state initialized");
            run_assertions(&mut harness, host);
            runtime.block_on(host.terminal.shutdown());
            drop(borrowed);
            harness.shut_down();
            Ok(())
        })
        .expect("run headless terminal test app");
}

fn is_node_focused(host: &TestHostState, node: NodeRef) -> bool {
    node.get_untracked()
        .is_some_and(|id| host.focused.get_untracked() == Some(id))
}

fn pump_frames(harness: &mut Harness<Box<dyn AppHandler>>) {
    for _ in 0..4 {
        harness.pump();
    }
}

fn press_key(
    harness: &mut Harness<Box<dyn AppHandler>>,
    event: KeyEvent,
    modifiers: Modifiers,
    stamp_ms: u64,
) {
    let timestamp = Timestamp::from_origin(Duration::from_millis(stamp_ms));
    harness.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: event.clone(),
        modifiers,
        timestamp,
    });
    harness.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Released,
        event,
        modifiers,
        timestamp: Timestamp::from_origin(Duration::from_millis(stamp_ms.saturating_add(5))),
    });
    pump_frames(harness);
}

fn type_text(harness: &mut Harness<Box<dyn AppHandler>>, text: &str, start_ms: u64) {
    for (idx, ch) in text.chars().enumerate() {
        let mut buf = [0_u8; 4];
        let s: &str = ch.encode_utf8(&mut buf);
        press_key(
            harness,
            KeyEvent::character(s),
            Modifiers::NONE,
            start_ms + idx as u64 * 10,
        );
    }
}

fn wait_for_pty(harness: &mut Harness<Box<dyn AppHandler>>, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !ready() {
        assert!(Instant::now() < deadline, "PTY response timed out");
        harness.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn screen(session: &Session) -> String {
    use alacritty_terminal::{
        grid::Dimensions,
        index::{Column, Line},
    };
    session.with_term(|term| {
        let grid = term.grid();
        (0..term.screen_lines())
            .flat_map(|row| {
                (0..term.columns()).map(move |column| grid[Line(row as i32)][Column(column)].c)
            })
            .collect()
    })
}

#[test]
fn single_terminal_respects_its_hosts_key_map() {
    let options = SpawnOptions {
        cwd: std::env::temp_dir(),
        args: vec!["-c".into(), "exit 0".into()],
        ..SpawnOptions::default()
    };
    run_headless_terminal(options, Preferences::default(), |harness, host| {
        host.outer_ref.focus();
        pump_frames(harness);
        press_key(harness, KeyEvent::character("w"), Modifiers::CONTROL, 10);
        assert_eq!(host.workspace_actions.get(), 1);

        host.terminal.focus();
        pump_frames(harness);
        assert!(is_node_focused(host, host.terminal_ref));
        press_key(harness, KeyEvent::character("w"), Modifiers::CONTROL, 20);
        press_key(harness, KeyEvent::character(","), Modifiers::CONTROL, 30);
        press_key(harness, KeyEvent::character("`"), Modifiers::CONTROL, 40);
        press_key(
            harness,
            KeyEvent::named(NamedKey::F6, PhysicalKey::Code(KeyCode::F6)),
            Modifiers::NONE,
            50,
        );
        assert_eq!(
            host.workspace_actions.get(),
            1,
            "host close must not steal shell Ctrl+W"
        );
        assert_eq!(
            (host.settings.get(), host.toggles.get(), host.exits.get()),
            (1, 1, 1)
        );
    });
}

#[test]
fn tab_routes_to_pty_and_outside_terminal_traversal_still_works() {
    let options = SpawnOptions {
        cwd: std::env::temp_dir(),
        args: vec![
            "-c".into(),
            "stty -echo -icanon min 1 time 0; printf '\\033]0;ready\\007'; dd bs=1 count=4 2>/dev/null | od -An -tx1".into(),
        ],
        ..SpawnOptions::default()
    };
    run_headless_terminal(options, Preferences::default(), |harness, host| {
        wait_for_pty(harness, || {
            host.terminal.session().and_then(|s| s.title()).as_deref() == Some("ready")
        });

        host.outer_ref.focus();
        pump_frames(harness);
        let tab = KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab));
        press_key(harness, tab.clone(), Modifiers::NONE, 10);
        assert!(
            is_node_focused(host, host.next_ref),
            "root Tab must still traverse outside the terminal"
        );
        press_key(harness, tab.clone(), Modifiers::SHIFT, 20);
        assert!(
            is_node_focused(host, host.outer_ref),
            "root reverse traversal must still work"
        );

        host.terminal.focus();
        pump_frames(harness);
        assert!(is_node_focused(host, host.terminal_ref));
        press_key(harness, tab.clone(), Modifiers::NONE, 30);
        assert!(
            is_node_focused(host, host.terminal_ref),
            "Tab must not leave terminal input"
        );
        press_key(harness, tab, Modifiers::SHIFT, 40);
        assert!(
            is_node_focused(host, host.terminal_ref),
            "Shift+Tab must not leave terminal input"
        );

        let session = host.terminal.session().expect("session running");
        wait_for_pty(harness, || screen(&session).contains("09 1b 5b 5a"));
    });
}

#[test]
fn tab_completes_a_filename_in_a_real_interactive_shell() {
    let account_shell = nix::unistd::User::from_uid(nix::unistd::Uid::current())
        .expect("test account lookup")
        .map(|user| user.shell);
    let Some(bash) = account_shell
        .into_iter()
        .chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|path| path.join("bash")),
        )
        .find(|path| {
            path.file_name().is_some_and(|name| name == "bash")
                && std::process::Command::new(path)
                    .args(["--noprofile", "--norc", "-c", "type bind >/dev/null 2>&1"])
                    .status()
                    .is_ok_and(|status| status.success())
        })
    else {
        return;
    };

    let directory = tempfile::tempdir().expect("completion directory");
    std::fs::write(
        directory.path().join("gds-completion-token"),
        "COMPLETION_SUCCEEDED\n",
    )
    .expect("completion fixture");

    let options = SpawnOptions {
        cwd: directory.path().to_owned(),
        shell: Some(bash),
        args: vec!["--noprofile".into(), "--norc".into(), "-i".into()],
        ..SpawnOptions::default()
    };

    run_headless_terminal(options, Preferences::default(), |harness, host| {
        wait_for_pty(harness, || host.terminal.session().is_some());
        let session = host.terminal.session().expect("interactive bash session");
        session
            .write(b"PS1=''; stty -echo; bind '\"\\C-i\": complete'; printf '\\033]0;ready\\007'\r")
            .expect("initialize isolated readline session");
        wait_for_pty(harness, || session.title().as_deref() == Some("ready"));

        host.terminal.focus();
        pump_frames(harness);
        type_text(harness, "cat gds-completion-t", 20);
        press_key(
            harness,
            KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
            Modifiers::NONE,
            300,
        );
        assert!(is_node_focused(host, host.terminal_ref));
        press_key(
            harness,
            KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
            Modifiers::NONE,
            320,
        );
        wait_for_pty(harness, || {
            screen(&session).contains("COMPLETION_SUCCEEDED")
        });
    });
}

#[test]
fn changing_terminal_typography_updates_existing_grid_metrics() {
    let options = SpawnOptions {
        cwd: std::env::temp_dir(),
        args: vec!["-c".into(), "exit 0".into()],
        ..SpawnOptions::default()
    };
    run_headless_terminal(options, Preferences::default(), |harness, host| {
        pump_frames(harness);
        let before = host.terminal.test_geometry();
        host.terminal.apply_preferences(Preferences {
            font_family: ".SystemMonospace".into(),
            font_size: 24.0,
            ..Preferences::default()
        });
        pump_frames(harness);
        let after = host.terminal.test_geometry();
        assert!(after.cell_width > before.cell_width);
        assert!(after.cell_height > before.cell_height);
        assert!(
            after.columns < before.columns,
            "larger text must reduce grid columns"
        );
        assert!(
            after.rows < before.rows,
            "larger text must reduce grid rows"
        );
    });
}

#[test]
fn cursor_blink_only_runs_while_focused_and_allowed() {
    let options = SpawnOptions {
        cwd: std::env::temp_dir(),
        args: vec!["-c".into(), "sleep 60".into()],
        ..SpawnOptions::default()
    };
    run_headless_terminal(options, Preferences::default(), |harness, host| {
        assert_eq!(host.terminal.test_cursor_state(), (false, false, true));

        host.terminal.focus();
        pump_frames(harness);
        assert_eq!(host.terminal.test_cursor_state(), (true, true, true));

        harness.advance(BLINK_INTERVAL);
        pump_frames(harness);
        assert_eq!(host.terminal.test_cursor_state(), (true, true, false));

        host.terminal.apply_preferences(Preferences {
            cursor_blink: false,
            ..Preferences::default()
        });
        pump_frames(harness);
        assert_eq!(host.terminal.test_cursor_state(), (true, false, true));

        host.terminal.apply_preferences(Preferences::default());
        pump_frames(harness);
        assert_eq!(host.terminal.test_cursor_state(), (true, true, true));

        harness.advance(BLINK_INTERVAL);
        pump_frames(harness);
        assert!(!host.terminal.test_cursor_state().2);

        host.gds.appearance().motion().set_enabled(false);
        pump_frames(harness);
        assert_eq!(host.terminal.test_cursor_state(), (true, false, true));

        host.gds.appearance().motion().set_enabled(true);
        pump_frames(harness);
        assert_eq!(host.terminal.test_cursor_state(), (true, true, true));

        host.next_ref.focus();
        pump_frames(harness);
        assert_eq!(host.terminal.test_cursor_state(), (false, false, true));
    });
}

#[test]
fn terminal_grid_renders_tabs_and_combining_marks() {
    let options = SpawnOptions {
        cwd: std::env::temp_dir(),
        args: vec![
            "-c".into(),
            "printf '%s\\r\\n' 'A\te\u{0301}DONE'; sleep 60".into(),
        ],
        ..SpawnOptions::default()
    };
    run_headless_terminal(options, Preferences::default(), |harness, host| {
        wait_for_pty(harness, || {
            host.terminal
                .session()
                .is_some_and(|session| screen(&session).contains("DONE"))
        });
        host.terminal.focus();
        pump_frames(harness);
        assert!(host.terminal.test_geometry().columns > 0);
    });
}

#[test]
fn internal_file_drag_pointer_up_bubbles_past_terminal_to_paste_escaped_paths() {
    let options = SpawnOptions {
        cwd: std::env::temp_dir(),
        args: vec![
            "-c".into(),
            "printf '\\033]0;ready\\007'; IFS= read -r line; printf 'DROPPED:%s\\n' \"$line\"; sleep 60".into(),
        ],
        ..SpawnOptions::default()
    };
    run_headless_terminal(options, Preferences::default(), |harness, host| {
        wait_for_pty(harness, || {
            host.terminal.session().and_then(|s| s.title()).as_deref() == Some("ready")
        });

        host.drag.begin(
            vec![
                PathBuf::from("/tmp/alpha file.txt"),
                PathBuf::from("/tmp/beta'quote.rs"),
            ],
            10.0,
            5.0,
        );
        let drop_pos = Point::new(CssPx(200.0), CssPx(200.0));
        harness.deliver_to_first(SurfaceEvent::Pointer {
            action: PointerAction::Moved,
            event: PointerEvent::mouse(drop_pos),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::from_origin(Duration::from_millis(10)),
        });
        harness.deliver_to_first(SurfaceEvent::Pointer {
            action: PointerAction::Released,
            event: PointerEvent::mouse(drop_pos).with_button(PointerButton::Primary),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::from_origin(Duration::from_millis(20)),
        });
        pump_frames(harness);

        assert!(
            !host.drag.is_active().get_untracked(),
            "PointerUp over TerminalView must bubble to host drop target during active drag"
        );
        assert!(is_node_focused(host, host.terminal_ref));

        press_key(
            harness,
            KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
            Modifiers::NONE,
            40,
        );
        let session = host.terminal.session().expect("session running");
        wait_for_pty(harness, || {
            let text = screen(&session);
            text.contains("DROPPED:'/tmp/alpha file.txt'") && text.contains("beta")
        });
    });
}

#[test]
fn reactive_signal_types_remain_unsync() {
    let owner = zgui::reactive::Owner::new();
    owner.with(|| {
        let sig: zgui::reactive::RwSignal<usize, LocalStorage> =
            zgui::reactive::RwSignal::new_local(1);
        assert_eq!(sig.get_untracked(), 1);
    });
    owner.cleanup();
}
