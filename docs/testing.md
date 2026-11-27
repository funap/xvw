# Testing Guide

## Test boundaries

`xvw` is currently a binary-only GPUI application. The default test suite is
therefore kept inside the binary so that adding an integration test does not
compile the entire UI as a second crate target.

Use these locations consistently:

| Test kind | Location | Rule |
| --- | --- | --- |
| Small unit tests, including private implementation details | Next to the implementation in `src/**/*.rs` | Keep the `#[cfg(test)] mod tests` block local to the module. |
| Large unit-test suites for one subsystem | A sibling file such as `src/core/structure/tests.rs` | Declare it with `#[cfg(test)] mod tests;` in the owning module. |
| Pure UI/layout helpers | A sibling file such as `src/ui/views/hex_view/layout_tests.rs` | Test calculations without starting a GPUI application. |
| Stable input fixtures | `testdata/` | Load with `include_str!`/`include_bytes!`; never use a developer-specific absolute path. |
| Public API integration tests | `tests/*.rs` | Add these only after the tested API is intentionally exposed from a library target. |
| GPUI/UI end-to-end tests | A separately opted-in target or crate | Do not make the default `cargo test` start a window or a full UI runtime. |

The distinction between an inline test module and a sibling test file is only
about size and readability. Both are unit tests and may access private APIs.
Do not move tests to `tests/` just to make them look more integrated.

## What belongs in the default suite

Default tests should be deterministic correctness checks:

- assert returned values, state transitions, error handling, and boundary behavior;
- use small but representative input sizes;
- avoid wall-clock assertions and benchmark labels;
- clean up files created by a test, or use a unique path in the system temp directory.

Performance measurements belong in a benchmark target or an explicitly opted-in
performance job. A slow test is not a reliable benchmark because debug mode,
machine load, and the test runner change its result.

## Commands

```text
cargo fmt --all -- --check
cargo test
cargo test core::structure::stream::tests
cargo test --no-run
```

The `[profile.test]` settings in `Cargo.toml` keep debug information out of
test artifacts. This reduces build size and startup time while preserving the
default unoptimized behavior needed by the correctness suite.

When the application is later split into a reusable library and a UI binary,
the core library can gain `tests/*.rs` integration tests. That migration should
be deliberate: keep private algorithm tests as unit tests and keep GPUI tests
out of the default core test target.
