/// ASCII banner rendered at startup above the chat history.
/// Keeping it ASCII-only ensures compatibility with our lint rules.
pub const MESSAGE_PREFIX: &str = "__SLIDE_ASCII_BANNER__\n";

pub const STARTUP_BANNER: &str = r"SLIDE CODE
  _____ _ _     _      _____          _      
 / ____| (_)   | |    / ____|        | |     
| (___ | |_  __| |___| |     ___   __| | ___ 
 \___ \| | |/ _` / __| |    / _ \ / _` |/ _ \
 ____) | | | (_| \__ \ |___| (_) | (_| |  __/
|_____/|_|_|\__,_|___/\_____\___/ \__,_|\___|
";

/// Build the banner message that can be pushed into the chat history list.
pub fn banner_message() -> String {
    format!("{MESSAGE_PREFIX}{STARTUP_BANNER}")
}
