mod form;

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use form::Form;
use gds::{
    app::Gds,
    i18n,
    service::{
        remote::{Address, ConnectOptions, HostKeyChallenge, Session},
        task_manager::{Owner as TaskOwner, Scope},
    },
};
use tokio::sync::{mpsc, oneshot};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Owner, RwSignal};
use zgui::view;
use zgui::view::install_stylesheet;
use zgui_ui::{
    ButtonProps, ButtonVariant, DialogContentProps, DialogFooterProps, DialogHeaderProps,
    DialogProps, DialogTitleProps,
};
use zgui_ui_primitives::Binding;

use super::model::{Intent, Request};

type Completion = Rc<dyn Fn(Result<Address, String>)>;

const DIALOG_SHEET: &str = "gds-files-connect-dialog";
const DIALOG_CSS: &str = r#"
.gds-connect-dialog {
    width: 480px;
    max-width: 92vw;
}

.gds-connect-dialog[data-host-key="true"] {
    width: 520px;
    max-width: 92vw;
}

.gds-connect-dialog__error {
    font-size: 13px;
    color: var(--gds-error);
}

.gds-connect-dialog__body {
    display: flex;
    flex-direction: column;
    gap: 10px;
}

.gds-connect-dialog__muted {
    font-size: 13px;
    color: var(--gds-muted-foreground);
}
"#;

#[derive(Clone)]
struct Attempt {
    intent: Intent,
    completed: Completion,
}

#[derive(Clone)]
struct Challenge {
    changed: bool,
    description: String,
    answer: Rc<RefCell<Option<oneshot::Sender<bool>>>>,
}

impl Challenge {
    fn answer(&self, trusted: bool) {
        if let Some(answer) = self.answer.borrow_mut().take() {
            let _ = answer.send(trusted);
        }
    }
}

#[derive(Clone)]
enum DialogState {
    Closed,
    Form {
        form: Form,
        attempt: Attempt,
        error: Option<String>,
    },
    Connecting {
        address: Address,
        attempt: Attempt,
        tasks: Rc<TaskOwner>,
        challenge: Option<Challenge>,
    },
}

struct Connection {
    address: Address,
    session: Session,
}

struct State {
    tasks: Scope,
    owner: Owner,
    dialog: RwSignal<DialogState, LocalStorage>,
    sessions: RefCell<HashMap<String, Connection>>,
}

#[derive(Clone)]
pub(crate) struct Connector(Rc<State>);

impl Connector {
    pub fn new(tasks: Scope) -> Self {
        Self(Rc::new(State {
            tasks,
            owner: Owner::current().expect("connector belongs to a window"),
            dialog: RwSignal::new_local(DialogState::Closed),
            sessions: RefCell::new(HashMap::new()),
        }))
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self.0.dialog.get(), DialogState::Connecting { .. })
    }

    pub fn pending(&self) -> Option<(Address, Intent)> {
        match self.0.dialog.get() {
            DialogState::Connecting {
                address, attempt, ..
            } => Some((address, attempt.intent)),
            _ => None,
        }
    }

    pub fn session(&self, address: &Address) -> Option<Session> {
        self.0
            .sessions
            .borrow()
            .get(&address.connection_key())
            .map(|connection| connection.session.clone())
    }

    pub fn connections(&self) -> Vec<Address> {
        let mut addresses = self
            .0
            .sessions
            .borrow()
            .values()
            .map(|connection| connection.address.clone())
            .collect::<Vec<_>>();
        addresses.sort_by(|a, b| a.uri().cmp(b.uri()));
        addresses
    }

    pub fn prompt(
        &self,
        address: Option<Address>,
        intent: Intent,
        completed: impl Fn(Result<Address, String>) + 'static,
    ) {
        if !matches!(self.0.dialog.get_untracked(), DialogState::Closed) {
            completed(Err(i18n::text("remote-task-running")));
            return;
        }
        self.0.owner.with(|| {
            let form = Form::new(address);
            self.0.dialog.set(DialogState::Form {
                form,
                attempt: Attempt {
                    intent,
                    completed: Rc::new(completed),
                },
                error: None,
            });
        });
    }

    pub fn cancel(&self) {
        let current = self.0.dialog.get_untracked();
        self.0.dialog.set(DialogState::Closed);
        let attempt = match current {
            DialogState::Closed => return,
            DialogState::Form { attempt, .. } => attempt,
            DialogState::Connecting {
                tasks,
                challenge,
                attempt,
                ..
            } => {
                if let Some(challenge) = challenge {
                    challenge.answer(false);
                }
                tasks.cancel();
                attempt
            }
        };
        (attempt.completed)(Err(i18n::text("remote-cancelled")));
    }

    fn submit(&self) {
        let DialogState::Form { form, attempt, .. } = self.0.dialog.get_untracked() else {
            return;
        };
        match form.request() {
            Ok(request) => self.begin(request, attempt),
            Err(error) => self.0.dialog.set(DialogState::Form {
                form,
                attempt,
                error: Some(i18n::text(error.label())),
            }),
        }
    }

    fn begin(&self, request: Request, attempt: Attempt) {
        let Some(dirs) = directories::BaseDirs::new() else {
            (attempt.completed)(Err(i18n::text("remote-state-unavailable")));
            self.0.dialog.set(DialogState::Closed);
            return;
        };
        let known_hosts = dirs
            .state_dir()
            .unwrap_or(dirs.data_local_dir())
            .join("gds/files/known_hosts");
        let Request {
            address,
            credentials,
        } = request;
        let tasks = Rc::new(self.0.tasks.named_child("Remote connection"));
        let weak = Rc::downgrade(&self.0);
        let challenge_tasks = tasks.clone();
        let (sender, receiver) = mpsc::channel::<HostKeyChallenge>(1);
        self.0.owner.with(|| {
            zgui::tokio::spawn_receiver(receiver, move |challenge| {
                let Some(state) = weak.upgrade() else {
                    return;
                };
                state.dialog.update(|dialog| {
                    if let DialogState::Connecting {
                        tasks,
                        challenge: pending,
                        ..
                    } = dialog
                        && Rc::ptr_eq(tasks, &challenge_tasks)
                    {
                        *pending = Some(Challenge {
                            changed: challenge.changed,
                            description: format!(
                                "{}:{}\n{}",
                                challenge.host, challenge.port, challenge.fingerprint
                            ),
                            answer: Rc::new(RefCell::new(Some(challenge.answer))),
                        });
                    }
                });
            });
        });

        let destination = address.clone();
        let intent = attempt.intent;
        let active_tasks: Scope = (*tasks).clone();
        let weak = Rc::downgrade(&self.0);
        self.0.dialog.set(DialogState::Connecting {
            address: address.clone(),
            attempt: attempt.clone(),
            tasks: tasks.clone(),
            challenge: None,
        });

        self.0.owner.with(|| {
            Gds::current().tasks().spawn_ui(
                "Connect to remote server",
                async move {
                    let work = active_tasks.spawn_cancellable_named(
                        "Connect to remote server",
                        async move {
                            let session = Session::connect(
                                &destination,
                                ConnectOptions {
                                    credentials,
                                    known_hosts,
                                },
                                sender,
                            )
                            .await?;
                            if intent == Intent::Add {
                                session.list(&destination).await?;
                            }
                            Ok::<_, gds::service::remote::Error>(session)
                        },
                    );
                    work.await
                },
                move |result| {
                    let Some(state) = weak.upgrade() else {
                        return;
                    };
                    let current = state.dialog.get_untracked();
                    if !matches!(
                        &current,
                        DialogState::Connecting { tasks: active, .. } if Rc::ptr_eq(active, &tasks)
                    ) {
                        return;
                    }
                    let outcome = result
                        .map_err(|error| error.to_string())
                        .and_then(|inner| inner.map_err(|error| error.to_string()))
                        .and_then(|inner| inner.map_err(|error| error.to_string()))
                        .map(|session| {
                            state.sessions.borrow_mut().insert(
                                address.connection_key(),
                                Connection {
                                    address: address.clone(),
                                    session,
                                },
                            );
                            address
                        });
                    state.dialog.set(DialogState::Closed);
                    (attempt.completed)(outcome);
                },
            );
        });
    }

    pub fn disconnect(
        &self,
        address: Address,
        finished: impl FnOnce(Result<(), String>) + 'static,
    ) {
        let Some(connection) = self
            .0
            .sessions
            .borrow_mut()
            .remove(&address.connection_key())
        else {
            finished(Ok(()));
            return;
        };
        let finished = RefCell::new(Some(finished));
        self.0.owner.with(|| {
            Gds::current().tasks().spawn_ui(
                "Close remote session",
                async move { connection.session.close().await },
                move |result| {
                    let result = result
                        .map_err(|error| error.to_string())
                        .and_then(|inner| inner.map_err(|error| error.to_string()));
                    if let Some(callback) = finished.borrow_mut().take() {
                        callback(result);
                    }
                },
            );
        });
    }

    pub fn view(&self) -> AnyView {
        install_stylesheet(DIALOG_SHEET, DIALOG_CSS);
        let dialog_state = self.0.dialog;
        let open = Signal::derive_local(move || !matches!(dialog_state.get(), DialogState::Closed));
        let cancel = self.clone();
        let connector = self.clone();
        AnyView::new(view! {
            Dialog(
                open = Binding::controlled(open, move |next: bool| {
                    if !next {
                        cancel.cancel();
                    }
                }),
            ) {
                DialogContent(class = "gds-connect-dialog") {
                    column(
                        attr:data-host-key = move || {
                            matches!(
                                dialog_state.get(),
                                DialogState::Connecting {
                                    challenge: Some(_),
                                    ..
                                }
                            )
                            .then(|| "true".to_owned())
                        },
                    ) {
                        {render_dialog(connector.clone())}
                    }
                }
            }
        })
    }
}

fn render_dialog(connector: Connector) -> AnyView {
    let dialog_state = connector.0.dialog;
    AnyView::new(move || match dialog_state.get() {
        DialogState::Closed => AnyView::new(view! { box() }),
        DialogState::Form {
            form,
            attempt,
            error,
        } => {
            let cancel = connector.clone();
            let submit = connector.clone();
            let enter = connector.clone();
            let ready = form.clone();
            let (title, label) = match attempt.intent {
                Intent::Add => ("remote-add-location", "remote-add"),
                Intent::Open => ("remote-connect", "remote-connect-action"),
            };
            AnyView::new(view! {
                column(class = "gds-connect-dialog__body") {
                    DialogHeader {
                        DialogTitle {{i18n::text(title)}}
                    }
                    {form.view(move || enter.submit())}
                    {error.map(|message| view! {
                        text(class = "gds-connect-dialog__error") {{message}}
                    }.into_view())}
                    DialogFooter {
                        Button(
                            variant = ButtonVariant::Outline,
                            on:click = move |_| cancel.cancel(),
                        ) {{i18n::text("dialog-cancel")}}
                        {ready.ready().then(|| {
                            let submit = submit.clone();
                            view! {
                                Button(
                                    variant = ButtonVariant::Default,
                                    on:click = move |_| submit.submit(),
                                ) {{i18n::text(label)}}
                            }.into_view()
                        })}
                    }
                }
            })
        }
        DialogState::Connecting {
            address,
            challenge: None,
            ..
        } => {
            let cancel = connector.clone();
            AnyView::new(view! {
                column(class = "gds-connect-dialog__body") {
                    DialogHeader {
                        DialogTitle {{i18n::text("remote-connect")}}
                    }
                    text {{address.to_string()}}
                    text(class = "gds-connect-dialog__muted") {{i18n::text("remote-connecting")}}
                    DialogFooter {
                        Button(
                            variant = ButtonVariant::Outline,
                            on:click = move |_| cancel.cancel(),
                        ) {{i18n::text("dialog-cancel")}}
                    }
                }
            })
        }
        DialogState::Connecting {
            challenge: Some(challenge),
            ..
        } => {
            let cancel = connector.clone();
            let trust_connector = connector.clone();
            let trust = challenge.clone();
            let title = if challenge.changed {
                "remote-host-changed"
            } else {
                "remote-host-new"
            };
            AnyView::new(view! {
                column(class = "gds-connect-dialog__body") {
                    DialogHeader {
                        DialogTitle {{i18n::text(title)}}
                    }
                    text {{challenge.description}}
                    DialogFooter {
                        Button(
                            variant = ButtonVariant::Outline,
                            on:click = move |_| cancel.cancel(),
                        ) {{i18n::text("dialog-cancel")}}
                        Button(
                            variant = ButtonVariant::Default,
                            on:click = move |_| {
                                trust.answer(true);
                                trust_connector.0.dialog.update(|dialog| {
                                    if let DialogState::Connecting { challenge, .. } = dialog {
                                        *challenge = None;
                                    }
                                });
                            },
                        ) {{i18n::text("remote-host-trust")}}
                    }
                }
            })
        }
    })
}
