# Repository Guidelines

## Project Structure & Module Organization
- `slide-rs/` hosts the Rust workspace; key crates span `core/` (session orchestration), `common/` (config IO helpers), `cli/` (CLI entrypoint), `tui/` (ratatui UI), and `chatgpt/` (HTTP client wrapper).
- `slide-cli/` contains the Node launcher that ships the compiled binary; generated presentations land in `slides/`, and architectural notes live under `docs/`.
- Tests colocate with crates via `#[cfg(test)]` modules, with broader integration coverage under `slide-rs/tests/` and fixtures in `tests/fixtures/`.

## Build, Test, and Development Commands
- `npm run dev` — compile the Rust workspace and hot-reload the CLI for interactive development.
- `./slide.sh` — wrapper around the same build-and-run loop; use when you need a single command for end-to-end runs.
- `npm run build` or `cd slide-rs && cargo build --release` — generate release binaries.
- `npm run test` (mirrors `cargo test`) — execute the full suite; run at the repo root for CI parity.

## Coding Style & Naming Conventions
- Target Rust 2021 with four-space indentation; keep modules and functions snake_case, and types or traits CamelCase.
- Favor `Result` flows over `unwrap`/`expect`; surface context with error enums or `anyhow`-style helpers already in use.
- Run `cargo fmt` and `cargo clippy --all-targets` before sending reviews to keep formatting and linting consistent.

## Testing Guidelines
- Name tests after observable behavior (`renders_help_modal`, `loads_config_defaults`).
- Keep tests deterministic: mock network calls and stage inputs via `tests/fixtures/`.
- Prefer unit tests beside code guarded by `#[cfg(test)]`; use `slide-rs/tests/` for integration scenarios.

## Commit & Pull Request Guidelines
- Write commit subjects in imperative mood (`tui: add command palette`), one concern per commit; add context in the body when behavior changes.
- PRs should explain motivation, link related issues, note follow-ups, and attach UI artifacts (screenshots or asciinema) when behavior is visible.

## Security & Configuration Tips
- Assume restricted networks; avoid adding online dependencies without justification and capture secrets via `SlideConfig` or existing helpers.
- Limit filesystem writes to workspace directories and prefer crate-level feature flags when introducing new capabilities.
