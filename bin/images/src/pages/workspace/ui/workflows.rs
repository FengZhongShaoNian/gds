//! Asynchronous user workflows for document lifecycle, editing, clipboard, export, and settings.

pub(in crate::pages::workspace::ui) use super::*;

pub mod clipboard;
pub mod document_io;
pub mod editing;
pub mod file_ops;
pub mod settings;
