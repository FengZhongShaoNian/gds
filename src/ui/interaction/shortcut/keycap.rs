use zgui::vocab::{Key, NamedKey};

pub(super) fn display_key(key: &Key) -> String {
    match key {
        Key::Named(named) => match *named {
            NamedKey::Escape => "Esc".to_owned(),
            NamedKey::ArrowUp => "↑".to_owned(),
            NamedKey::ArrowDown => "↓".to_owned(),
            NamedKey::ArrowLeft => "←".to_owned(),
            NamedKey::ArrowRight => "→".to_owned(),
            other => other.as_str().to_owned(),
        },
        Key::Character(text) | Key::Other(text) => {
            if text.as_str() == " " {
                "Space".to_owned()
            } else {
                text.as_str().to_uppercase()
            }
        }
        Key::Dead(Some(ch)) => ch.to_uppercase().to_string(),
        Key::Dead(None) | Key::Unidentified => "?".to_owned(),
        _ => "?".to_owned(),
    }
}
