use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use zgui::reactive::{Mounted, flush, install, prelude::*};

use super::{
    DirectoryPicker, DirectoryPickerView, Entry, Location, MIN_SCROLLABLE_ROWS, Page, Request,
};

struct PendingReply {
    send: Option<tokio::sync::oneshot::Sender<Result<Page, String>>>,
}

fn controlled_loader() -> (super::Loader, Rc<RefCell<Vec<(Request, PendingReply)>>>) {
    let queue: Rc<RefCell<Vec<(Request, PendingReply)>>> = Rc::new(RefCell::new(Vec::new()));
    let q_clone = Rc::clone(&queue);
    let loader: super::Loader = Rc::new(
        move |req: Request| -> Pin<Box<dyn Future<Output = Result<Page, String>>>> {
            let (tx, rx) = tokio::sync::oneshot::channel();
            q_clone
                .borrow_mut()
                .push((req, PendingReply { send: Some(tx) }));
            Box::pin(async move {
                match rx.await {
                    Ok(res) => res,
                    Err(_) => Err("cancelled".to_owned()),
                }
            })
        },
    );
    (loader, queue)
}

fn make_page(
    path: &str,
    parent: Option<&str>,
    names: &[(&str, bool)],
    next_cursor: Option<&str>,
) -> Page {
    Page {
        location: Location {
            path: path.to_owned(),
            label: path.rsplit('/').next().unwrap_or(path).to_owned(),
        },
        parent: parent.map(str::to_owned),
        ancestors: parent
            .map(|p| {
                vec![Location {
                    path: p.to_owned(),
                    label: "root".to_owned(),
                }]
            })
            .unwrap_or_default(),
        entries: names
            .iter()
            .map(|(name, hidden)| Entry {
                location: Location {
                    path: format!("{path}/{name}"),
                    label: (*name).to_owned(),
                },
                hidden: *hidden,
            })
            .collect(),
        next_cursor: next_cursor.map(str::to_owned),
    }
}

#[test]
fn directory_picker_types_compile() {
    let _ = (DirectoryPicker::new, DirectoryPickerView);
}

#[test]
fn navigation_history_and_selection_work_end_to_end() {
    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        let (loader, pending) = controlled_loader();
        let picker = DirectoryPicker::with_path(loader, "/home");
        flush();

        assert!(picker.is_loading().get_untracked());
        assert_eq!(pending.borrow().len(), 1);
        let (_, mut reply) = pending.borrow_mut().remove(0);
        reply
            .send
            .take()
            .expect("sender")
            .send(Ok(make_page(
                "/home",
                Some("/"),
                &[
                    ("Documents", false),
                    ("Downloads", false),
                    (".config", true),
                ],
                None,
            )))
            .ok();
        flush();

        assert!(!picker.is_loading().get_untracked());
        assert_eq!(picker.current_path().as_deref(), Some("/home"));
        assert_eq!(picker.rows().get_untracked().len(), 2);

        picker.toggle_hidden();
        assert_eq!(picker.rows().get_untracked().len(), 3);

        picker.set_filter("conf");
        assert_eq!(picker.rows().get_untracked().len(), 1);
        picker.select_row(0);
        assert_eq!(picker.selected_path().as_deref(), Some("/home/.config"));

        picker.navigate("/home/Documents");
        flush();
        let (_, mut reply2) = pending.borrow_mut().remove(0);
        reply2
            .send
            .take()
            .expect("sender")
            .send(Ok(make_page("/home/Documents", Some("/home"), &[], None)))
            .ok();
        flush();

        assert!(picker.can_go_back().get_untracked());
        assert!(!picker.can_go_forward().get_untracked());

        picker.back();
        flush();
        let (req, mut reply_back) = pending.borrow_mut().remove(0);
        assert_eq!(req.path.as_deref(), Some("/home"));
        reply_back
            .send
            .take()
            .expect("sender")
            .send(Ok(make_page(
                "/home",
                Some("/"),
                &[("Documents", false)],
                None,
            )))
            .ok();
        flush();

        assert!(!picker.can_go_back().get_untracked());
        assert!(picker.can_go_forward().get_untracked());
    });
    scope.unmount();
}

#[test]
fn stale_async_load_is_cancelled_when_user_navigates_away() {
    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        let (loader, pending) = controlled_loader();
        let picker = DirectoryPicker::with_path(loader, "/slow");
        flush();

        let (_, mut slow_reply) = pending.borrow_mut().remove(0);
        picker.navigate("/fast");
        flush();

        // Resolving the superseded request must not overwrite `/fast`.
        assert!(slow_reply.send.take().expect("sender").is_closed());
        let (req, mut fast_reply) = pending.borrow_mut().remove(0);
        assert_eq!(req.path.as_deref(), Some("/fast"));
        fast_reply
            .send
            .take()
            .expect("sender")
            .send(Ok(make_page("/fast", Some("/"), &[("Work", false)], None)))
            .ok();
        flush();

        assert_eq!(picker.current_path().as_deref(), Some("/fast"));
    });
    scope.unmount();
}

#[test]
fn pagination_prefetches_while_below_min_scrollable_rows() {
    install().ok();
    let scope = Mounted::new();
    scope.with(|| {
        let calls = Rc::new(Cell::new(0));
        let calls_ref = Rc::clone(&calls);
        let loader: super::Loader = Rc::new(move |req: Request| {
            let count = calls_ref.get() + 1;
            calls_ref.set(count);
            Box::pin(async move {
                match req.cursor.as_deref() {
                    None => Ok(make_page(
                        "/root",
                        None,
                        &[("a", false), ("b", false)],
                        Some("c2"),
                    )),
                    Some("c2") => {
                        let extra: Vec<(&'static str, bool)> = vec![
                            ("d1", false),
                            ("d2", false),
                            ("d3", false),
                            ("d4", false),
                            ("d5", false),
                            ("d6", false),
                        ];
                        Ok(make_page("/root", None, &extra, None))
                    }
                    _ => Ok(make_page("/root", None, &[], None)),
                }
            })
        });

        let picker = DirectoryPicker::with_path(loader, "/root");
        for _ in 0..8 {
            flush();
        }

        assert!(picker.rows().get_untracked().len() >= MIN_SCROLLABLE_ROWS);
        assert_eq!(calls.get(), 2);
    });
    scope.unmount();
}
