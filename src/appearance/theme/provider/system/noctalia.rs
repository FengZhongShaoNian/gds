//! Decode palettes rendered by Noctalia v5's template engine.
use std::{collections::HashMap, path::Path};

use gpui_kit::Hsla;
use serde::Deserialize;

use super::super::color::hex_color;
use super::material::{self, Parsed};
use crate::appearance::theme::{Error, ModePreference, TerminalColors};

const SCHEMA_VERSION: u64 = 1;

#[derive(Deserialize)]
struct Export {
    #[serde(default)]
    mode: Option<serde_json::Value>,
    colors: HashMap<String, HashMap<String, String>>,
    #[serde(flatten)]
    extensions: HashMap<String, serde_json::Value>,
}

/// Resolve the requested UI and terminal scheme from a rendered Noctalia v5 export.
///
/// # Errors
/// Rejects malformed JSON, unsupported `schema_version`, invalid active mode when
/// following `System`, missing requested schemes, incomplete role pairs, or partial
/// terminal exports.
pub(super) fn parse(
    contents: &str,
    path: &Path,
    preference: ModePreference,
) -> Result<Parsed, Error> {
    let export: Export = serde_json::from_str(contents).map_err(|source| Error::Json {
        path: path.to_owned(),
        source,
    })?;
    if let Some(version) = export.extensions.get("schema_version")
        && version.as_u64() != Some(SCHEMA_VERSION)
    {
        return Err(Error::invalid(
            "schema_version",
            "unsupported Noctalia theme schema version",
        ));
    }
    let active = export.mode.as_ref().and_then(serde_json::Value::as_str);
    let mode = material::resolve_mode(active, preference, "Noctalia")?;
    let mut palette = material::palette(&export.colors, mode, "Noctalia")?;
    let name = material::scheme_name(mode);
    if let Some(terminal) = export.extensions.get("terminal") {
        // A present terminal section is atomic: never mix partial roles with fallbacks.
        let scheme = terminal
            .get(name)
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| {
                Error::invalid(
                    format!("terminal.{name}"),
                    "requested Noctalia terminal scheme is missing or not an object",
                )
            })?;
        palette.terminal = TerminalColors {
            foreground: terminal_role(scheme, name, "foreground")?,
            background: terminal_role(scheme, name, "background")?,
            cursor: terminal_role(scheme, name, "cursor")?,
            cursor_text: terminal_role(scheme, name, "cursor_text")?,
            selection: terminal_role(scheme, name, "selection")?,
            selection_text: terminal_role(scheme, name, "selection_text")?,
            normal: terminal_ansi(scheme, name, "normal")?,
            bright: terminal_ansi(scheme, name, "bright")?,
        };
    }
    Ok(Parsed { palette, mode })
}

/// # Errors
/// Rejects missing, non-string, or malformed hexadecimal terminal colors.
fn terminal_role(
    scheme: &serde_json::Map<String, serde_json::Value>,
    scheme_name: &str,
    role: &str,
) -> Result<Hsla, Error> {
    let field = format!("terminal.{scheme_name}.{role}");
    let value = scheme
        .get(role)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            Error::invalid(
                &field,
                "required Noctalia terminal color is missing or not a string",
            )
        })?;
    hex_color(value, &field, false)
}

/// # Errors
/// Rejects missing arrays, arrays without all 8 ANSI entries, or invalid hex colors.
fn terminal_ansi(
    scheme: &serde_json::Map<String, serde_json::Value>,
    scheme_name: &str,
    group: &str,
) -> Result<[Hsla; 8], Error> {
    let field = format!("terminal.{scheme_name}.{group}");
    let entries = scheme
        .get(group)
        .and_then(serde_json::Value::as_array)
        .filter(|entries| entries.len() == 8)
        .ok_or_else(|| {
            Error::invalid(
                &field,
                "expected an array of 8 Noctalia ANSI terminal colors",
            )
        })?;
    let mut colors = [Hsla::default(); 8];
    for (index, (slot, entry)) in colors.iter_mut().zip(entries).enumerate() {
        let item_field = format!("{field}[{index}]");
        let value = entry.as_str().ok_or_else(|| {
            Error::invalid(
                &item_field,
                "required Noctalia terminal color is missing or not a string",
            )
        })?;
        *slot = hex_color(value, &item_field, false)?;
    }
    Ok(colors)
}

#[cfg(test)]
mod tests;
