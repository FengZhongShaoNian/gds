use std::{collections::HashMap, path::Path};

use serde::Deserialize;

use super::super::color::hex_color;
use super::material::{self, Parsed};
use crate::appearance::theme::{Error, ModePreference};

#[derive(Deserialize)]
struct Export {
    // Only System needs this metadata; explicit choices accept any exported value.
    #[serde(default)]
    mode: Option<serde_json::Value>,
    colors: HashMap<String, HashMap<String, String>>,
    #[serde(flatten)]
    extensions: HashMap<String, serde_json::Value>,
}

/// Resolve the requested scheme from a DMS export, without evaluating QML.
/// System uses the export's active mode; explicit choices override it.
///
/// # Errors
/// Rejects malformed JSON, invalid active mode when following System, an absent
/// requested scheme, missing roles or invalid colors. No other scheme is substituted.
pub(super) fn parse(
    contents: &str,
    path: &Path,
    preference: ModePreference,
) -> Result<Parsed, Error> {
    let export: Export = serde_json::from_str(contents).map_err(|source| Error::Json {
        path: path.to_owned(),
        source,
    })?;
    let active = export.mode.as_ref().and_then(serde_json::Value::as_str);
    let mode = material::resolve_mode(active, preference, "DMS")?;
    let mut palette = material::palette(&export.colors, mode, "DMS")?;
    let name = material::scheme_name(mode);
    if let Some(dank16) = export.extensions.get("dank16") {
        // A present export is atomic: never blend a partial ANSI scheme with defaults.
        let mut ansi = [palette.terminal.foreground; 16];
        for (index, color) in ansi.iter_mut().enumerate() {
            let role = format!("color{index}");
            let field = format!("dank16.{role}.{name}");
            let value = dank16
                .get(&role)
                .and_then(|entry| entry.get(name))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    Error::invalid(
                        &field,
                        "required DMS terminal color is missing or not a string",
                    )
                })?;
            *color = hex_color(value, &field, false)?;
        }
        palette.terminal.normal = std::array::from_fn(|i| ansi[i]);
        palette.terminal.bright = std::array::from_fn(|i| ansi[i + 8]);
    }
    Ok(Parsed { palette, mode })
}

#[cfg(test)]
mod tests;
