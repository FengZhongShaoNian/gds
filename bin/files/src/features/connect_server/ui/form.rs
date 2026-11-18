use std::rc::Rc;

use gds::i18n;
use gds::service::remote::{Address, Credentials, Protocol};
use gds::ui::IconButtonProps;
use gds::ui::icon::{IconName, IconProps};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Owner, RenderEffect, RwSignal, on_cleanup_local};
use zgui::view::install_stylesheet;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};
use zgui_ui::{ButtonProps, ButtonVariant, InputProps};

use super::super::model::{DEFAULT_SFTP_PORT, DEFAULT_SMB_PORT, Details, Error, Request};

const FORM_SHEET: &str = "gds-files-connect-form";
const FORM_CSS: &str = r#"
.gds-connect-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
    min-width: 0;
}

.gds-connect-form__header {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
}

.gds-connect-form__row {
    display: flex;
    flex-direction: row;
    gap: 12px;
    width: 100%;
    min-width: 0;
}

.gds-connect-form__host {
    flex: 1 1 0%;
    min-width: 0;
}

.gds-connect-form__port {
    width: 100px;
    flex-shrink: 0;
}

.gds-connect-form__field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
}

.gds-connect-form__label {
    font-size: 12px;
    color: var(--gds-muted-foreground);
}

.gds-connect-form__methods {
    display: flex;
    flex-direction: row;
    flex-wrap: wrap;
    gap: 6px;
}

.gds-connect-form__protocol-choice {
    width: 100%;
    height: auto;
    padding: 12px;
    justify-content: flex-start;
}

.gds-connect-form__protocol-body {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-width: 0;
}

.gds-connect-form__protocol-text {
    display: flex;
    flex-direction: column;
    flex: 1 1 0%;
    min-width: 0;
    gap: 4px;
}

.gds-connect-form__protocol-title {
    font-weight: 600;
}

.gds-connect-form__protocol-desc {
    font-size: 12px;
    color: var(--gds-muted-foreground);
}
"#;

#[derive(Clone, PartialEq)]
pub(super) enum Authentication {
    Agent,
    Password(RwSignal<String, LocalStorage>),
    Key {
        path: RwSignal<String, LocalStorage>,
        passphrase: RwSignal<String, LocalStorage>,
    },
    Guest,
}

impl Authentication {
    fn label(&self, protocol: Protocol) -> &'static str {
        match self {
            Self::Agent => "remote-auth-agent",
            Self::Password(_) if protocol == Protocol::Smb => "remote-auth-user",
            Self::Password(_) => "remote-password",
            Self::Key { .. } => "remote-auth-key",
            Self::Guest => "remote-auth-guest",
        }
    }

    fn same_method(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Clone, PartialEq)]
struct Fields {
    protocol: Protocol,
    host: RwSignal<String, LocalStorage>,
    host_node: NodeRef,
    port: RwSignal<String, LocalStorage>,
    path: RwSignal<String, LocalStorage>,
    user: RwSignal<String, LocalStorage>,
    authentication: RwSignal<Authentication, LocalStorage>,
    methods: Vec<Authentication>,
}

#[derive(Clone, PartialEq)]
pub(super) struct Form {
    owner: Owner,
    fields: RwSignal<Option<Fields>, LocalStorage>,
}

impl Fields {
    fn new(protocol: Protocol, address: Option<&Address>) -> Self {
        let user = address.map(Address::username).unwrap_or_default();
        let user = if user.is_empty() && protocol == Protocol::Sftp {
            std::env::var("USER").unwrap_or_default()
        } else {
            user
        };
        let password = Authentication::Password(RwSignal::new_local(String::new()));
        let (authentication, methods) = match protocol {
            Protocol::Sftp => (
                Authentication::Agent,
                vec![
                    Authentication::Agent,
                    password,
                    Authentication::Key {
                        path: RwSignal::new_local(String::new()),
                        passphrase: RwSignal::new_local(String::new()),
                    },
                ],
            ),
            Protocol::Smb => (
                if user.eq_ignore_ascii_case("guest") {
                    Authentication::Guest
                } else {
                    password.clone()
                },
                vec![password, Authentication::Guest],
            ),
        };
        Self {
            protocol,
            host_node: NodeRef::new(),
            host: RwSignal::new_local(
                address
                    .map(|address| address.host().to_owned())
                    .unwrap_or_default(),
            ),
            port: RwSignal::new_local(
                address
                    .map(Address::port)
                    .unwrap_or(match protocol {
                        Protocol::Sftp => DEFAULT_SFTP_PORT,
                        Protocol::Smb => DEFAULT_SMB_PORT,
                    })
                    .to_string(),
            ),
            path: RwSignal::new_local(address.map(Address::path).unwrap_or_else(|| {
                if protocol == Protocol::Sftp {
                    "/".into()
                } else {
                    String::new()
                }
            })),
            user: RwSignal::new_local(user),
            authentication: RwSignal::new_local(authentication),
            methods,
        }
    }

    fn request(&self) -> Result<Request, Error> {
        let authentication = self.authentication.get_untracked();
        let user = if matches!(authentication, Authentication::Guest) {
            "guest".to_owned()
        } else {
            self.user.get_untracked()
        };
        if user.is_empty() {
            return Err(Error::User);
        }
        let address = Details {
            protocol: self.protocol,
            host: self.host.get_untracked(),
            port: self.port.get_untracked(),
            user,
            path: self.path.get_untracked(),
        }
        .address()?;
        let credentials = match authentication {
            Authentication::Agent | Authentication::Guest => Credentials::default(),
            Authentication::Password(password) => Credentials {
                password: password.get_untracked(),
                ..Default::default()
            },
            Authentication::Key { path, passphrase } => {
                let path = std::path::PathBuf::from(path.get_untracked());
                if !path.is_absolute() {
                    return Err(Error::Key);
                }
                let passphrase = passphrase.get_untracked();
                Credentials {
                    private_key: Some(path),
                    key_passphrase: (!passphrase.is_empty()).then_some(passphrase),
                    ..Default::default()
                }
            }
        };
        Ok(Request {
            address,
            credentials,
        })
    }

    fn view(&self, choose_protocol: impl Fn() + 'static, submit: Rc<dyn Fn()>) -> AnyView {
        install_stylesheet(FORM_SHEET, FORM_CSS);
        let host_node = self.host_node;
        let focus_effect = RenderEffect::new(move |prev: Option<bool>| {
            let mounted = host_node.get().is_some();
            if mounted && prev != Some(true) {
                host_node.focus();
                host_node.select_all();
            }
            mounted
        });
        on_cleanup_local(move || drop(focus_effect));

        let auth = self.authentication;
        let protocol = self.protocol;
        let (protocol_label, path_label, user_label) = match protocol {
            Protocol::Sftp => ("SFTP", "remote-folder", "remote-username"),
            Protocol::Smb => ("SMB", "remote-share-path", "remote-smb-user"),
        };

        let methods_view = self
            .methods
            .iter()
            .map(move |method| {
                let for_click = method.clone();
                let for_selected = method.clone();
                let label = i18n::text(method.label(protocol));
                let active =
                    Signal::derive_local(move || auth.with(|c| c.same_method(&for_selected)));
                view! {
                    Button(
                        variant = ButtonVariant::Outline,
                        a11y:selected = active,
                        attr:data-selected = move || active.get().then(|| "true".to_owned()),
                        on:click = move |_| auth.set(for_click.clone()),
                    ) {{label}}
                }
                .into_view()
            })
            .collect::<Vec<_>>();

        let host_sig = self.host;
        let port_sig = self.port;
        let path_sig = self.path;
        let user_sig = self.user;
        let submit_host = submit.clone();
        let submit_port = submit.clone();
        let submit_path = submit.clone();

        AnyView::new(view! {
            column(class = "gds-connect-form") {
                row(class = "gds-connect-form__header") {
                    IconButton(
                        icon = IconName::ArrowLeft,
                        label = i18n::text("remote-change-protocol"),
                        on:click = move |_| choose_protocol(),
                    )
                    text(class = "gds-connect-form__protocol-title") {{protocol_label}}
                }
                row(class = "gds-connect-form__row") {
                    box(class = "gds-connect-form__host") {
                        FormField(label = i18n::text("remote-host")) {
                            Input(
                                value = host_sig,
                                node_ref = host_node,
                                placeholder = "server.example.com",
                                on:key_down = move |ev| {
                                    if ev.key == Key::Named(NamedKey::Enter) && ev.modifiers.is_empty() {
                                        ev.prevent_default();
                                        submit_host();
                                    }
                                },
                            )
                        }
                    }
                    box(class = "gds-connect-form__port") {
                        FormField(label = i18n::text("remote-port")) {
                            Input(
                                value = port_sig,
                                on:key_down = move |ev| {
                                    if ev.key == Key::Named(NamedKey::Enter) && ev.modifiers.is_empty() {
                                        ev.prevent_default();
                                        submit_port();
                                    }
                                },
                            )
                        }
                    }
                }
                FormField(label = i18n::text(path_label)) {
                    Input(
                        value = path_sig,
                        on:key_down = move |ev| {
                            if ev.key == Key::Named(NamedKey::Enter) && ev.modifiers.is_empty() {
                                ev.prevent_default();
                                submit_path();
                            }
                        },
                    )
                }
                column(class = "gds-connect-form__field") {
                    text(class = "gds-connect-form__label") {{i18n::text("remote-authentication")}}
                    row(class = "gds-connect-form__methods") {
                        {methods_view}
                    }
                }
                {move || {
                    let submit_inner = submit.clone();
                    match auth.get() {
                        Authentication::Agent => AnyView::new(view! {
                            FormField(label = i18n::text(user_label)) {
                                Input(
                                    value = user_sig,
                                    on:key_down = move |ev| {
                                        if ev.key == Key::Named(NamedKey::Enter) && ev.modifiers.is_empty() {
                                            ev.prevent_default();
                                            submit_inner();
                                        }
                                    },
                                )
                            }
                        }),
                        Authentication::Password(password) => {
                            let submit_pw = submit_inner.clone();
                            AnyView::new(view! {
                                column(class = "gds-connect-form") {
                                    FormField(label = i18n::text(user_label)) {
                                        Input(value = user_sig)
                                    }
                                    FormField(label = i18n::text("remote-password")) {
                                        Input(
                                            value = password,
                                            secure = true,
                                            on:key_down = move |ev| {
                                                if ev.key == Key::Named(NamedKey::Enter) && ev.modifiers.is_empty() {
                                                    ev.prevent_default();
                                                    submit_pw();
                                                }
                                            },
                                        )
                                    }
                                }
                            })
                        }
                        Authentication::Key { path, passphrase } => {
                            let submit_key = submit_inner.clone();
                            AnyView::new(view! {
                                column(class = "gds-connect-form") {
                                    FormField(label = i18n::text(user_label)) {
                                        Input(value = user_sig)
                                    }
                                    FormField(label = i18n::text("remote-auth-key")) {
                                        Input(
                                            value = path,
                                            placeholder = "/home/user/.ssh/id_ed25519",
                                        )
                                    }
                                    FormField(label = i18n::text("remote-passphrase")) {
                                        Input(
                                            value = passphrase,
                                            secure = true,
                                            on:key_down = move |ev| {
                                                if ev.key == Key::Named(NamedKey::Enter) && ev.modifiers.is_empty() {
                                                    ev.prevent_default();
                                                    submit_key();
                                                }
                                            },
                                        )
                                    }
                                }
                            })
                        }
                        Authentication::Guest => AnyView::new(view! { box() }),
                    }
                }}
            }
        })
    }
}

#[component]
fn FormField(#[prop(into)] label: String, children: Children) -> impl IntoView {
    view! {
        column(class = "gds-connect-form__field") {
            text(class = "gds-connect-form__label") {{label}}
            {children.into_view_once()}
        }
    }
}

impl Form {
    pub fn new(address: Option<Address>) -> Self {
        let owner = Owner::current().expect("connection form belongs to a window");
        Self {
            owner,
            fields: RwSignal::new_local(
                address
                    .as_ref()
                    .map(|address| Fields::new(address.protocol(), Some(address))),
            ),
        }
    }

    pub fn ready(&self) -> bool {
        self.fields.with(Option::is_some)
    }

    pub fn request(&self) -> Result<Request, Error> {
        self.fields
            .get_untracked()
            .ok_or(Error::Protocol)?
            .request()
    }

    pub fn view(&self, submit: impl Fn() + 'static) -> AnyView {
        let submit: Rc<dyn Fn()> = Rc::new(submit);
        let fields = self.fields;
        let owner = self.owner.clone();
        AnyView::new(move || match fields.get() {
            Some(current) => current.view(move || fields.set(None), submit.clone()),
            None => {
                let outer_owner = owner.clone();
                let options = [
                    (Protocol::Sftp, "SFTP", "remote-sftp-description"),
                    (Protocol::Smb, "SMB", "remote-smb-description"),
                ]
                .into_iter()
                .map(|(protocol, label, description)| {
                    let click_owner = outer_owner.clone();
                    view! {
                        Button(
                            variant = ButtonVariant::Ghost,
                            class = "gds-connect-form__protocol-choice",
                            a11y:label = label,
                            on:click = move |_| {
                                click_owner.with(|| {
                                    fields.set(Some(Fields::new(protocol, None)));
                                });
                            },
                        ) {
                            row(class = "gds-connect-form__protocol-body") {
                                column(class = "gds-connect-form__protocol-text") {
                                    text(class = "gds-connect-form__protocol-title") {{label}}
                                    text(class = "gds-connect-form__protocol-desc") {{i18n::text(description)}}
                                }
                                Icon(name = IconName::ChevronRight)
                            }
                        }
                    }
                    .into_view()
                })
                .collect::<Vec<_>>();
                AnyView::new(view! {
                    column(class = "gds-connect-form") {
                        text(class = "gds-connect-form__label") {{i18n::text("remote-choose-protocol")}}
                        {options}
                    }
                })
            }
        })
    }
}
