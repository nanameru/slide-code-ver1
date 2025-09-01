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
