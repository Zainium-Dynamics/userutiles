// ops/mod.rs — Operations module root.
//
// Sub-modules:
// rename — Intra-filesystem atomic rename(2) and renameat2 wrappers
// crossdev — Cross-device move: parallel copy + verify + delete
// atomic — High-level atomic_exchange / atomic_no_replace orchestration

pub mod atomic;
pub mod crossdev;
pub mod rename;
