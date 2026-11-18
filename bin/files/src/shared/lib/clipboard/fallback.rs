//! Phase 3B text-clipboard shim framing `# gds-files-v1:` payloads over `zgui`'s text clipboard.

use serde::{Deserialize, Serialize};
use url::Url;

use super::{
    Error, Identity, Mode, Snapshot,
    protocol::{Formats, MAX_PAYLOAD_BYTES},
};

const METADATA_PREFIX: &str = "# gds-files-v1:";
const REMOTE_URI_HEADER: &str = "# gds-files-remote-v1:";

#[derive(Serialize, Deserialize)]
struct Metadata {
    mode: Mode,
    identity: String,
}

pub(super) struct TextCodec;

impl TextCodec {
    /// Encode a local file clipboard snapshot into a UTF-8 text payload prefixed with
    /// `# gds-files-v1:` metadata and RFC 2483 `file://` URIs.
    ///
    /// # Errors
    /// Rejects invalid file paths or oversized payloads.
    pub(super) fn encode_snapshot(snapshot: &Snapshot) -> Result<String, Error> {
        let uris = String::from_utf8(Formats::encode_uris(&snapshot.paths)?)
            .expect("URL serialization always produces UTF-8");
        let identity = snapshot
            .identity
            .0
            .as_deref()
            .expect("private encoding requires an identity");
        let metadata = serde_json::to_string(&Metadata {
            mode: snapshot.mode,
            identity: identity.to_owned(),
        })?;
        let text = format!("{METADATA_PREFIX}{metadata}\r\n{uris}");
        if text.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::TooLarge);
        }
        Ok(text)
    }

    /// Encode remote URIs into a GDS-tagged URI list so plain clipboard text is never
    /// mistaken for copyable file URIs.
    ///
    /// # Errors
    /// Rejects empty, malformed, password-bearing, or oversized URI lists.
    pub(super) fn encode_uris(uris: &[String]) -> Result<String, Error> {
        let checked = Formats::parse_uri_list(&uris.join("\n"))?;
        let text = format!("{REMOTE_URI_HEADER}\r\n{}\r\n", checked.join("\r\n"));
        if text.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::TooLarge);
        }
        Ok(text)
    }

    /// Decode a local file snapshot from text, never inferring files from ordinary text.
    ///
    /// # Errors
    /// Rejects malformed metadata, non-local URIs, or oversized payloads.
    pub(super) fn decode_snapshot(text: Option<&str>) -> Result<Option<Snapshot>, Error> {
        let Some(text) = text else {
            return Ok(None);
        };
        if text.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::TooLarge);
        }
        let Some((header, body)) = text.split_once('\n') else {
            return Ok(None);
        };
        let Some(json) = header.trim_end_matches('\r').strip_prefix(METADATA_PREFIX) else {
            return Ok(None);
        };
        let metadata: Metadata = serde_json::from_str(json)?;
        let mut snapshot = Formats::decode_uris(body.as_bytes(), metadata.mode)?;
        snapshot.identity = Identity::decode(metadata.identity.as_bytes())?;
        Ok(Some(snapshot))
    }

    /// Decode local file URIs or tagged remote URIs from clipboard text.
    ///
    /// # Errors
    /// Rejects malformed metadata or URI lists.
    pub(super) fn decode_uris(text: Option<&str>) -> Result<Option<(Mode, Vec<String>)>, Error> {
        if let Some(snapshot) = Self::decode_snapshot(text)? {
            let uris = snapshot
                .paths
                .iter()
                .map(|path| {
                    Url::from_file_path(path)
                        .map(|uri| uri.to_string())
                        .map_err(|_| Error::InvalidPath(path.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(Some((snapshot.mode, uris)));
        }
        let Some(text) = text else {
            return Ok(None);
        };
        if text.len() > MAX_PAYLOAD_BYTES {
            return Err(Error::TooLarge);
        }
        let Some((header, body)) = text.split_once('\n') else {
            return Ok(None);
        };
        if header.trim_end_matches('\r') != REMOTE_URI_HEADER {
            return Ok(None);
        }
        let uris = Formats::parse_uri_list(body)?;
        Ok(Some((Mode::Copy, uris)))
    }
}
