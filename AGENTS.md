# Repository Guidelines

## Project Structure & Module Organization
slide-rs/ anchors the Rust workspace. cli/ wires the compiled crates into the command-line entrypoint, tui/ renders the ratatui terminal UI, core/ owns session orchestration, tool dispatch, and safety constraints, common/ centralizes config IO helpers, and chatgpt/ wraps the HTTP client. slide-cli/ is the Node launcher for distributing the compiled binary. Generated outputs live under slides/, while docs/ stores architectural notes and RFCs. Use slide.sh for local end-to-end runs; keep crate-level features isolated when adding new modules.

## Build, Test, and Development Commands
- `npm run dev` – builds the Rust workspace and hot-reloads the CLI.
- `./slide.sh` – convenience wrapper for the same build-and-run loop.
- `npm run build` or `cd slide-rs && cargo build --release` – produce release artifacts.
- `npm run test` mirrors `cargo test`; run from repository root for CI parity.

## Coding Style & Naming Conventions
Target Rust 2021 with four-space indentation. Modules, functions, and files use snake_case; types and traits use CamelCase. Prefer expressive error handling: avoid unwrap/expect and rely on Result flows. Keep format strings inline (no uninlined_format_args). Comment sparingly, focusing on intent and non-obvious control flow.

## Testing Guidelines
Default to `cargo test` inside slide-rs/. Unit tests sit beside code behind `#[cfg(test)]`; integration flows can live under slide-rs/tests/. Name tests after observable behavior (e.g., `renders_help_modal`). Keep tests deterministic—mock network calls and stage fixtures under tests/fixtures/. Run the full suite before pushing.

## Commit & Pull Request Guidelines
Commit summaries stay in imperative mood (`tui: add command palette with filter`). Limit each commit to a single concern and explain motivation or context in the body when it is not obvious. PRs should outline the change, rationale, reproduction steps, and any UI artifacts (screenshots or asciinema). Link related issues and note follow-ups or migration guidance when behavior changes.

## Security & Configuration Tips
Assume restricted networks; do not introduce new online dependencies without justification. Write secrets through SlideConfig or existing config helpers—never hardcode tokens. Keep writes within workspace directories to respect sandboxing.
