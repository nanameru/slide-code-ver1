use super::radar_animation;
use ratatui::text::Line;

/// Marker prefix embedded in chat history strings so the chat widget can
/// recognise the banner entry and render it with full styling.
pub const MESSAGE_PREFIX: &str = "__SLIDE_ASCII_BANNER__";

/// Build the banner message that can be pushed into the chat history list.
/// The message acts as a sentinel token that the chat widget expands into
/// the richly styled banner at render time.
pub fn banner_message() -> String {
    MESSAGE_PREFIX.to_string()
}

/// Lines used to render the banner both in terminal scrollback and inside the
/// chat widget.
pub fn banner_lines() -> Vec<Line<'static>> {
    radar_animation::frame_lines(0)
}

/// Lines rendered into the terminal scrollback at startup.
pub fn banner_history_lines() -> Vec<Line<'static>> {
    banner_lines()
}
