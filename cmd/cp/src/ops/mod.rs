// ops/mod.rs — Operations module root.
//
// Sub-modules:
// file — Single-file copy engine: reflink / sparse / copy_file_range / buffered,
// always landing through a temp-file + rename atomic overwrite.
// tree — Recursive directory-tree discovery and parallel copy.

pub mod file;
pub mod tree;
