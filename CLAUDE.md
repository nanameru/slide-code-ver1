# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Core Architecture

**Slide Code Test** is a terminal-based AI agent built in Rust that generates markdown slides through interactive chat. The architecture follows a hybrid approach with a Node.js launcher and Rust core:

```
slide-cli (Node.js) → slide-rs/cli → slide-rs/tui → slide-rs/core
                                 ↘ slide-rs/chatgpt (OpenAI GPT-5)
```

### Key Components

- **slide-cli/**: Node.js launcher that handles platform detection and binary execution
- **slide-rs/**: Main Rust workspace with multiple focused crates:
  - **cli**: Entry point binary with HTTP log viewer (port 6060)
  - **tui**: Terminal UI using ratatui 0.29 with interactive chat interface
  - **core**: AI tool execution engine with sandboxed command execution
  - **chatgpt**: OpenAI API integration (defaults to GPT-5 model)
  - **common**: Shared utilities and types
  - **ansi-escape**: ANSI to ratatui color conversion
  - **apply-patch**: File patching functionality
  - **arg0**: Binary dispatch system
  - **protocol**: Communication protocol types

### AI Tool Execution System

The core implements a codex-style tool execution system:
- **Exec Tool**: Sandboxed shell command execution with approval flows
- **Apply Patch Tool**: File modification via unified diff patches
- **Safety**: Command allowlist system (`is_safe_command.rs`) with auto-approval for read-only operations
- **Approval Manager**: Interactive user confirmation for potentially dangerous operations

## Development Commands

### Primary Development Workflow
```bash
# Quick development run (builds and starts TUI)
npm run dev
# OR
./slide.sh

# Manual Rust build
cd slide-rs && cargo build --release

# Run tests
npm run test
# OR
cd slide-rs && cargo test

# Check compilation
cargo check
```

### TUI Application Usage
```bash
# Interactive slide generation mode
slide

# Preview existing markdown slides
slide preview slides/sample.md
```

### Package Management
```bash
# Install as global CLI tool
npm run install-global

# Remove global installation
npm run uninstall-global
```

## Environment Configuration

### Required Environment Variables
```bash
# In .env.local or env.local
OPENAI_API_KEY=your_api_key_here
SLIDE_APP=1  # Enables Slide mode
```

The CLI automatically searches for env.local files in:
- Current working directory
- Parent directories (up to 3 levels)
- Executable location and parents

### Optional Environment Variables
```bash
SLIDE_MODEL=gpt-5  # Override default model
SLIDE_APPROVAL_MODE=untrusted|on-failure|on-request|never
```

## Code Standards & Safety

### Clippy Configuration
The workspace enforces strict safety rules:
```toml
[workspace.lints.clippy]
expect_used = "deny"
unwrap_used = "deny"
uninlined_format_args = "deny"
```

### Error Handling Pattern
Always use `anyhow::Result` for error propagation:
```rust
pub async fn process_tool_call() -> Result<Output> {
    // Never use .unwrap() or .expect()
    let result = fallible_operation()?;
    Ok(result)
}
```

### Command Safety Classes
- **Safe (auto-approved)**: `ls`, `cat`, `grep`, `git status`, `git diff`, `head`, `tail`
- **Dangerous (approval required)**: `rm`, `mv`, `chmod`, `sudo`, `curl`, network operations

## Architecture Patterns

### Async Runtime
All components use Tokio for async execution:
```rust
use tokio::process::Command;
// Use tokio::process instead of std::process for non-blocking execution
```

### TUI Color System
Consistent color theming via ratatui:
```rust
Style::default().fg(Color::Green)    // Success/additions
Style::default().fg(Color::Red)      // Errors/deletions
Style::default().fg(Color::Blue)     // Info/headers
Style::default().fg(Color::Yellow)   // Warnings/proposals
```

### Modular Crate Design
Each functional area is isolated into focused crates with minimal public APIs. The `core/lib.rs` deliberately keeps exports minimal to ensure clean boundaries.

## Debugging & Monitoring

### Log Viewer
The CLI automatically starts a web-based log viewer at `http://127.0.0.1:6060/` showing real-time logs from `/tmp/slide.log`.

### Debug Environment Variables
```bash
RUST_LOG=debug cargo run
RUST_LOG=slide_core=trace cargo run  # Component-specific logging
```

## Testing Strategy

### Unit Tests
```bash
cargo test                    # Run all tests
cargo test -p slide-core     # Test specific crate
```

### Integration Testing
```bash
# Test TUI interactions (uses insta for snapshot testing)
cargo test -p slide-tui
```

## Platform Support

- **Primary**: macOS, Linux
- **Secondary**: Windows (via PowerShell in shell configuration)
- **Distribution**: Cross-platform via Node.js launcher with platform-specific Rust binaries

## Current Development Status

### Implemented
- ✅ Interactive TUI with chat interface
- ✅ Markdown slide preview with keyboard navigation
- ✅ AI tool execution with sandboxing
- ✅ Command safety classification system
- ✅ GPT-5 API integration with streaming
- ✅ HTTP log viewer for debugging
- ✅ Cross-platform Node.js launcher

### In Progress
- 🔄 MCP (Model Context Protocol) support
- 🔄 One-shot slide generation mode
- 🔄 Enhanced error reporting and recovery

The codebase prioritizes safety, modularity, and interactive terminal experiences while maintaining high performance through Rust's async capabilities.