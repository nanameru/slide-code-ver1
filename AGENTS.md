# Repository Guidelines

## Project Structure & Module Organization
- `slide-rs/` (Rust workspace):
  - `cli/` (CLI entrypoints), `tui/` (terminal UI), `core/` (session/tools/safety scaffolding), `common/` (config/utils), `chatgpt/` (provider), others.
- `slide-cli/` (Node launcher): starts the compiled Rust binary; see `bin/slide.js`.
- `slides/` (generated Markdown slides), `docs/` (design notes), `slide.sh` (local dev runner).

## Build, Test, and Development Commands
- Local dev: `npm run dev` or `./slide.sh` (builds Rust, launches CLI).
- Build (release): `npm run build` or `cd slide-rs && cargo build --release`.
- Test: `npm run test` or `cd slide-rs && cargo test`.
- Global link (dev): `npm run install-global` / `npm run uninstall-global`.

## Coding Style & Naming Conventions
- Rust 2021; 4‑space indentation; follow idiomatic Rust (snake_case for modules/functions, CamelCase for types).
- Lints: workspace clippy denies `expect_used`, `unwrap_used`, `uninlined_format_args`.
- Keep changes minimal and focused; avoid unrelated refactors; prefer small, composable modules per crate.

## Testing Guidelines
- Use `cargo test`; add `#[cfg(test)]` unit tests near code or dedicated `tests/` when integration is clearer.
- Name tests by behavior, e.g. `renders_help_modal`, `summarizes_command_kind`.
- Keep tests fast and hermetic; avoid network I/O. Prefer fixtures under crate‑local `tests/fixtures/` when needed.

## Commit & Pull Request Guidelines
- Commits: concise imperative subject, scoped body (what/why). Example: `tui: add command palette with filter`.
- One logical change per commit when practical; include file paths or examples if helpful.
- PRs: summary of changes, rationale, screenshots/asciinema for TUI changes, reproduction steps for fixes, and any follow‑ups.
- Link related issues; note breaking changes and migration steps.

## Security & Configuration Tips
- Default runs restrict network in some environments; keep features working offline when possible.
- File writes should target workspace (`slides/`, crate dirs). Avoid writing outside unless explicitly required.
- Store secrets via config (`slide-common::SlideConfig`); never hard‑code API keys.

## Architecture Overview (Brief)
- Rust workspace with clear boundaries: `core` (session/tools/safety), `tui` (ratatui UI), `cli` (entry), `common` (config/IO), `chatgpt` (API). The Node launcher (`slide-cli`) is for developer‑friendly startup.

