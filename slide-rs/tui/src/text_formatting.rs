use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Truncate a tool result to fit within the given height and width. If the text is valid JSON, we format it in a compact way before truncating.
/// This is a best-effort approach that may not work perfectly for text where 1 grapheme is rendered as multiple terminal cells.
pub(crate) fn format_and_truncate_tool_result(
    text: &str,
    max_lines: usize,
    line_width: usize,
) -> String {
    // Work out the maximum number of graphemes we can display for a result.
    // It's not guaranteed that 1 grapheme = 1 cell, so we subtract 1 per line as a fudge factor.
    // It also won't handle future terminal resizes properly, but it's an OK approximation for now.
    let max_graphemes = (max_lines * line_width).saturating_sub(max_lines);

    if let Some(formatted_json) = format_json_compact(text) {
        truncate_text(&formatted_json, max_graphemes)
    } else {
        truncate_text(text, max_graphemes)
    }
}

/// Format JSON text in a compact single-line format with spaces for better Ratatui wrapping.
/// Ex: `{"a":"b",c:["d","e"]}` -> `{"a": "b", "c": ["d", "e"]}`
/// Returns the formatted JSON string if the input is valid JSON, otherwise returns None.
/// This is a little complicated, but it's necessary because Ratatui's wrapping is *very* limited
/// and can only do line breaks at whitespace. If we use the default serde_json format, we get lines
/// without spaces that Ratatui can't wrap nicely. If we use the serde_json pretty format as-is,
/// it's much too sparse and uses too many terminal rows.
/// Relevant issue: https://github.com/ratatui/ratatui/issues/293
pub(crate) fn format_json_compact(text: &str) -> Option<String> {
    let json = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let json_pretty = serde_json::to_string_pretty(&json).unwrap_or_else(|_| json.to_string());

    // Convert multi-line pretty JSON to compact single-line format by removing newlines and excess whitespace
    let mut result = String::new();
    let mut chars = json_pretty.chars().peekable();
    let mut in_string = false;
    let mut escape_next = false;

    // Iterate over the characters in the JSON string, adding spaces after : and , but only when not in a string
    while let Some(ch) = chars.next() {
        match ch {
            '"' if !escape_next => {
                in_string = !in_string;
                result.push(ch);
            }
            '\\' if in_string => {
                escape_next = !escape_next;
                result.push(ch);
            }
            '\n' | '\r' if !in_string => {
                // Skip newlines when not in a string
            }
            ' ' | '\t' if !in_string => {
                // Add a space after : and , but only when not in a string
                if let Some(&next_ch) = chars.peek() {
                    if let Some(last_ch) = result.chars().last() {
                        if (last_ch == ':' || last_ch == ',') && !matches!(next_ch, '}' | ']') {
                            result.push(' ');
                        }
                    }
                }
            }
            _ => {
                if escape_next && in_string {
                    escape_next = false;
                }
                result.push(ch);
            }
        }
    }

    Some(result)
}

/// Truncate `text` to `max_graphemes` graphemes. Using graphemes to avoid accidentally truncating in the middle of a multi-codepoint character.
pub(crate) fn truncate_text(text: &str, max_graphemes: usize) -> String {
    let mut graphemes = text.grapheme_indices(true);

    // Check if there's a grapheme at position max_graphemes (meaning there are more than max_graphemes total)
    if let Some((byte_index, _)) = graphemes.nth(max_graphemes) {
        // There are more than max_graphemes, so we need to truncate
        if max_graphemes >= 3 {
            // Truncate to max_graphemes - 3 and add "..." to stay within limit
            let mut truncate_graphemes = text.grapheme_indices(true);
            if let Some((truncate_byte_index, _)) = truncate_graphemes.nth(max_graphemes - 3) {
                let truncated = &text[..truncate_byte_index];
                format!("{truncated}...")
            } else {
                text.to_string()
            }
        } else {
            // max_graphemes < 3, so just return first max_graphemes without "..."
            let truncated = &text[..byte_index];
            truncated.to_string()
        }
    } else {
        // There are max_graphemes or fewer graphemes, return original text
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_text() {
        let text = "Hello, world!";
        let truncated = truncate_text(text, 8);
        assert_eq!(truncated, "Hello...");
    }

    #[test]
    fn test_format_json_compact_simple_object() {
        let json = r#"{ "name": "John", "age": 30 }"#;
        let result = format_json_compact(json).unwrap();
        assert_eq!(result, r#"{"name": "John", "age": 30}"#);
    }
}