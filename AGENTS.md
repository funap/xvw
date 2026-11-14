# xvw — Agent Instructions

Canonical repository guidance for `xvw`, a binary-only Rust 2024 GUI hex editor built on `gpui_kit` and Tokio. Direct user requests take precedence over this file.

## Working workflow

- Inspect `git status` and target files before editing; preserve unrelated user changes.
- Search first (`rg`) and read targeted line ranges rather than loading huge UI files (`src/ui/views/hex_view/mod.rs`, `src/ui/workspace/mod.rs`, `src/ui/workspace/action_router.rs`) in full.
- Keep diffs focused and strictly scoped. Never run `git reset`, `git checkout --`, or broad/destructive deletions to discard work.
- Iterate with narrow checks (`cargo check`, `cargo test <filter>`), then run the full verification gate before handoff.

## Commands

```text
cargo check --all-targets
cargo test <filter>          # focused inner-loop check (e.g. core::structure::tests)
cargo test --no-run
cargo fmt --all -- --check   # apply with `cargo fmt --all`; rustfmt.toml (max_width = 160) is authoritative
cargo clippy --all-targets   # maintain zero warnings
cargo test                   # full deterministic unit suite
cargo build --release
cargo run -- [path]          # optional flags: --ksy, --diff, --folder, --panel, --sidebar/--no-sidebar
```

`[profile.test]` in `Cargo.toml` sets `opt-level = 0` and `debug = 0` for fast incremental test builds.

## Architecture & layer boundaries

- **`src/core/` (pure engine)**: Deterministic domain logic — `Buffer`/`memmap2`, `Document`, undo `History`, `Command`s, `Editor`, layout/search/structure state, `AddressMap`, format codecs, and Kaitai `.ksy` parsing. Keep GPUI windows, UI widgets, and Tokio/filesystem I/O out of `core/` (prefer putting `gpui_kit::Global` impls in `src/ui/mod.rs` or `src/app_state.rs`; `ByteColoring`/`ByteCategoryColors` in `src/core/radix.rs` are existing exceptions).
- **`src/service/` (orchestration & async)**: `DocumentService`, `StructureService`, `SearchService`, `DiffService`, `BookmarkService`, and `ScratchService`. Bridges `core/` with `gpui_kit` (`App`, `Entity`, `Task`, `BackgroundExecutor`) and Tokio (`spawn_blocking`, `Notify`).
- **`src/ui/` (GPUI presentation)**: `gpui_kit` views, panels, dialogs, pane tree, menus, and `Workspace`. Observes/subscribes to entities and globals; delegates business logic to `core/` and async I/O or parsing to `service/`.
- **`gpui_kit` vs `gpui`**: Build UI and globals against `gpui_kit` (`gpui_kit::*`, `gpui_kit::prelude::*`, `gpui_kit::component::*`), not raw `gpui::` directly (only low-level graphics/color helpers such as `TransformationMatrix`, `Font`, and `transparent_black` use `gpui::`).

## Change routing map

| Concern | Primary locations |
| --- | --- |
| Entry, Tokio runtime, CLI flags, assets | `src/main.rs`, `src/launch_args.rs`, `src/assets.rs` |
| App services & globals | `src/app_state.rs` (`AppState`, `InsertModeState`, `PendingCompareState`), `src/ui/mod.rs` |
| Settings & themes | `src/settings.rs` (TOML persistence), `src/theme.rs`, `assets/themes/*.json` |
| Actions & keybindings | `src/actions.rs`, `src/keybindings.rs` (global), `src/ui/workspace/mod.rs`, `src/ui/views/hex_view/actions.rs` |
| Workspace routing, modals, menus | `src/ui/workspace/action_router.rs`, `src/ui/workspace/dialog_flow.rs`, `src/ui/menus.rs`, `src/ui/dialogs/` |
| Hex view & testable layout math | `src/ui/views/hex_view/` (`layout.rs`, `paint.rs`, `row_cache.rs`, `input_controller.rs`, `scroll_controller.rs`) |
| Import/export & copy formats | `src/core/format.rs`, `src/core/format/` (Raw, Intel HEX, Motorola S-Record, Base64, copy codecs) |
| Kaitai `.ksy` structure parsing | `src/core/structure/`, `src/service/structure_service.rs`, `docs/structure-definition-spec.md`, `testdata/structure/` |

Runtime structure loading uses `LoadStructureDefinition` (`cmd-shift-s` on macOS, `ctrl-shift-s` elsewhere). Supported `.ksy` syntax is documented in `docs/structure-definition-spec.md`.

## Implementation conventions

### GPUI & concurrency

- Create entities with `cx.new`; wire reactive updates with `cx.observe` / `cx.subscribe`; call `cx.notify()` after state mutations that affect rendering. GPUI state flow is entity/subscription based, not React/Elm.
- Keep the UI thread non-blocking: use `cx.spawn` for app tasks, `cx.spawn_in(window, ...)` when tied to a window, and `BackgroundExecutor` or Tokio `spawn_blocking` for file I/O, CPU-heavy work, and heavy `Arc` drops (see `crate::core::dealloc::discard_in_background`).
- Keep `RwLock` / `Mutex` scopes explicit and minimal; never hold a lock guard across `.await`.
- Keep layout, formatting, and state-transition helpers pure so they can be unit-tested without starting a GPUI window.

### Rust

- Follow Rust 2024 idioms, `snake_case`/`CamelCase`/`SCREAMING_SNAKE_CASE` naming, and existing module boundaries.
- Propagate recoverable errors with `Result`/`anyhow::Result`; use `expect` only for established invariants with an explanatory message.
- Avoid unnecessary clones or allocations in hot rendering, layout, and search paths.
- Add `///` doc comments to new public APIs.

## Domain invariants & gotchas

- **`BufferOffset` vs `PhysicalAddress`**: Sparse formats (Intel HEX, Motorola S-Record) can have address gaps and non-zero base addresses. Index contiguous buffer bytes with `BufferOffset` and map source/display addresses via `Document::address_map` (`src/core/address_map.rs`); never conflate them.
- **Document mutations & layout cache**: Route byte edits through `Command` / `Document::replace_bytes` (preserves undo `History`, `AddressMap`, `CustomLayoutRules`, `BookmarkStore`, and `is_read_only()`). Route bookmark and custom-layout edits through `BookmarkSessionMut` / `CustomLayoutSessionMut` (or bump `DocumentMetadata::layout_version` and invalidate `LayoutEngine`).
- **Line layout interactions**: Custom breaks, joins, empty lines, `AddressMap` segment breaks/gaps, bookmark folds, and inline structure headers/folds all feed `Editor::line_starts` (`src/core/editor/layout_engine.rs`, `src/core/layout.rs`). Preserve their interactions and existing tests.
- **Cancellable structure parsing**: `StructureService` + `ParseUpdateMailbox` stream and coalesce incremental `.ksy` updates off the UI thread; preserve progress reporting and cancellation behavior.
- **UTF-8 char boundaries**: Never slice `&str` at arbitrary byte offsets on non-ASCII text, search inputs, or paths without checking char boundaries (`is_char_boundary` or char-safe helpers).
- **Canonicalized paths**: `DocumentService` and `Workspace` canonicalize file paths; tests touching real filesystem paths must account for canonicalization.

## Testing policy

See `docs/testing.md` for full rules. Key constraints:

- Keep the default `cargo test` suite deterministic, fast, and headless inside the binary target.
- Put small private-API tests inline (`#[cfg(test)] mod tests`); put large suites or pure UI/layout helper tests in sibling files (`tests.rs`, `layout_tests.rs`) declared with `#[cfg(test)] mod ...;`, decoupled from GPUI runtime.
- Load fixtures from `testdata/` via `include_str!`/`include_bytes!`; never hardcode local paths or silently skip missing fixtures.
- Never add wall-clock performance assertions to `cargo test`.
- Do not add `src/lib.rs` or `tests/*.rs` solely to test this binary; a public integration-test surface requires an intentional library/UI split.

## Generated and local files

Do not commit or alter build, IDE, or agent metadata (`target/`, `.serena/`, `.vscode/`, `.idea/`, `.gemini/`, `.DS_Store`), and leave unrelated local fixtures untouched.
