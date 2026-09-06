//! Identity of a file within an analysis.

use serde::Serialize;

/// Index into the `files` array of an [`crate::analyze`] request.
///
/// No path and no namespace, just a number – paths change (an import can be
/// relative or aliased), this identifier doesn't. Every file-related table
/// in this crate is indexed by `FileId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct FileId(pub u32);
