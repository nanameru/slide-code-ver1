#[derive(Debug, Clone)]
pub struct ToolDef { pub name: String, pub description: String }

#[derive(Debug, Clone)]
pub struct ToolsConfig {
    pub include_plan_tool: bool,
    pub include_apply_patch_tool: bool,
    pub include_view_image_tool: bool,
    pub include_web_search_request: bool,
    pub use_streamable_shell_tool: bool,
}

#[derive(Debug, Clone)]
pub struct ToolsConfigParams {
    pub include_plan_tool: bool,
    pub include_apply_patch_tool: bool,
    pub include_view_image_tool: bool,
    pub include_web_search_request: bool,
    pub use_streamable_shell_tool: bool,
}

impl ToolsConfig {
    pub fn new(p: &ToolsConfigParams) -> Self {
        Self {
            include_plan_tool: p.include_plan_tool,
            include_apply_patch_tool: p.include_apply_patch_tool,
            include_view_image_tool: p.include_view_image_tool,
            include_web_search_request: p.include_web_search_request,
            use_streamable_shell_tool: p.use_streamable_shell_tool,
        }
    }
}

pub fn get_openai_tools(cfg: &ToolsConfig, _mcp_tools: Option<Vec<String>>) -> Vec<String> {
    let mut tools = Vec::new();
    if cfg.use_streamable_shell_tool { tools.push("exec_command".to_string()); }
    else { tools.push("shell".to_string()); }
    if cfg.include_apply_patch_tool { tools.push("apply_patch".to_string()); }
    if cfg.include_plan_tool { tools.push("update_plan".to_string()); }
    if cfg.include_view_image_tool { tools.push("view_image".to_string()); }
    if cfg.include_web_search_request { tools.push("web_search_request".to_string()); }
    tools
}

/// Render a concise instruction block that advertises available tools to the model.
/// This is a lightweight alternative to function/tool calling and mirrors codex style.
pub fn render_tools_instructions(cfg: &ToolsConfig, approval_mode_hint: Option<&str>) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("You can propose using the following tools by writing clear instructions:".to_string());
    if cfg.use_streamable_shell_tool {
        lines.push("- exec_command: run a shell command. Always explain why and prefer read-only commands (ls, cat, rg).".to_string());
    } else {
        lines.push("- shell: run a shell command. Always explain why and prefer read-only commands (ls, cat, rg).".to_string());
    }
    if cfg.include_apply_patch_tool {
        lines.push("- apply_patch: propose a unified diff to edit files. Keep edits minimal and correct.".to_string());
    }
    if cfg.include_plan_tool { lines.push("- update_plan: refine your task plan concisely.".to_string()); }
    if cfg.include_view_image_tool { lines.push("- view_image: request to view an image by path.".to_string()); }
    if cfg.include_web_search_request { lines.push("- web_search_request: request a web search when strictly necessary.".to_string()); }

    if let Some(mode) = approval_mode_hint {
        lines.push(format!("Approval policy: {mode}. Destructive or ambiguous actions may require user approval."));
    }

    lines.push("When proposing a tool, output a short rationale followed by the exact command or a minimal diff.".to_string());
    lines.join("\n")
}