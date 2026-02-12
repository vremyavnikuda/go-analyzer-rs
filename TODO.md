# TODO

## Analysis Logic
- [ ] Remove `goanalyzer/graph` command and server path if graph feature is no longer used.
- [ ] Improve `RaceLow` detection: avoid downgrading risk when unrelated synchronization exists elsewhere in goroutine.
- [ ] Replace `block_on` in async flow in `src/backend.rs` (around graph path) with fully async handling.

## UI
- [ ] Redesign combined signal display (`race` + `reassign`) to keep context clear without visual noise.
- [x] Refine hover race messages with read/write access context.

## Metadata
- [x] Remove stale badge for non-existing workflow (`rust.yml`) from `README.md`.
- [x] Fix incorrect image path in `vscode/README.md`.
