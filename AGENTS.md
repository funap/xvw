# xvw — Agent Instructions

Canonical repository guidance for `xvw`, a binary-only Rust 2024 GUI hex editor built on `gpui_kit` and Tokio. Direct user requests take precedence over this file.

## Workflow & Verification

- Preserve unrelated user changes; never run destructive git commands (`git reset`, `git checkout --`, etc.) to discard work.
- Search first (`rg`) and read targeted line ranges on large files rather than loading them in full.
- Do not commit or alter build, IDE, or agent metadata (`target/`, `.serena/`, `.vscode/`, `.idea/`, `.gemini/`, `.DS_Store`) or unrelated local fixtures.
- Iterate with narrow checks (`cargo check --all-targets`, `cargo test <filter>`), then run the full gate before handoff:

```text
cargo fmt --all -- --check   # format with `cargo fmt --all`; rustfmt.toml (max_width = 160) is authoritative
cargo clippy --all-targets   # maintain zero warnings
cargo test                   # full deterministic unit suite
```

## Commit Messages

- Use Conventional Commits in English: `<type>[(<scope>)]: <subject>` (lowercase imperative mood, no trailing period).
- Types: `feat`, `fix`, `refactor`, `docs`, `chore`, `test`, `perf`, `style`.
- Scope is optional; prefer `kebab-case` (e.g. `hex-view`, `deps`).
- Keep commits focused (one concern per commit) with a concise single-line subject; add a body only when non-obvious rationale is needed. Reserve `vX.Y.Z` for release commits.

## Architecture & Layer Boundaries

- **`src/core/` (pure engine)**: Deterministic domain logic (`Buffer`, `Document`, `History`, `Command`, `Editor`, `AddressMap`, format codecs, Kaitai `.ksy` parser). Keep GPUI/UI, Tokio, and filesystem I/O out of `core/` (background drop dispatch is registered from `service/`). Put `gpui_kit::Global` impls outside `core/` (`src/ui/`, `src/app_state.rs`, `src/settings.rs`).
- **`src/service/` (orchestration & async)**: Async tasks, file I/O, and background parsing/scanning bridging `core/` with `gpui_kit` and Tokio.
- **`src/ui/` (GPUI presentation)**: `gpui_kit` views, panels, dialogs, pane tree, menus, and `Workspace`. Use `gpui_kit` rather than raw `gpui::` directly (except low-level graphics/font/color types).
- **Non-blocking UI & GPUI state**: Wire reactive updates with `cx.new` / `cx.observe` / `cx.subscribe` / `cx.notify()`. Offload file I/O, CPU-heavy work, and heavy `Arc` drops (`crate::core::dealloc::discard_in_background`) off the UI thread. Keep layout/formatting helpers pure and unit-testable without a GPUI window.

## Domain Invariants & Gotchas

- **`BufferOffset` vs `PhysicalAddress`**: Sparse formats (Intel HEX, Motorola S-Record) can have gaps and non-zero bases. Index contiguous buffer bytes with `BufferOffset` and map display/source addresses via `Document::address_map`; never conflate them.
- **Document mutations & layout cache**: Route byte edits through `Command` / `Document::replace_bytes` (preserves undo `History`, `AddressMap`, `CustomLayoutRules`, `BookmarkStore`, `is_read_only()`). Route bookmark/custom-layout edits through `BookmarkSessionMut` / `CustomLayoutSessionMut` (or bump `DocumentMetadata::layout_version` and invalidate `LayoutEngine`).
- **Line layout interactions**: Custom breaks/joins/empty lines, `AddressMap` segment breaks/gaps, bookmark folds, and inline structure headers/folds all feed `Editor::line_starts`. Preserve their interactions and tests.
- **Cancellable structure parsing**: `StructureService` + `ParseUpdateMailbox` stream and coalesce incremental `.ksy` updates off the UI thread; preserve progress reporting and cancellation. Supported `.ksy` syntax is documented in `docs/structure-definition-spec.md`.
- **UTF-8 char boundaries**: Never slice `&str` at arbitrary byte offsets on non-ASCII text, search inputs, or paths without checking char boundaries.
- **Canonicalized paths**: Services and `Workspace` canonicalize file paths; tests touching the filesystem must account for canonicalization.

## Testing Policy

See `docs/testing.md` for full rules. Key constraints:

- Keep `cargo test` deterministic, fast, and headless inside the binary target; never add wall-clock timing assertions.
- Put unit tests inline (`#[cfg(test)] mod tests`) or in sibling files (`#[cfg(test)] mod ...;`) decoupled from GPUI runtime. Do not add `src/lib.rs` or `tests/*.rs` solely for testing.
- Load fixtures from `testdata/` via `include_str!`/`include_bytes!`; never hardcode local paths or silently skip missing fixtures.
