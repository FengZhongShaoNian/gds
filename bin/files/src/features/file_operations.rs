//! File mutations, immutable batches and operation history.
mod api;
#[allow(unused_imports)]
pub(crate) use api::{Batch, Error, Kind, Report, names, transfer};
pub(crate) mod paste;
pub(crate) mod remote;

pub(crate) mod history;
pub(crate) mod recycle;

#[cfg(unix)]
pub(crate) mod permissions;

#[allow(unused_imports)]
pub(crate) use api::{new_folder, rename};
