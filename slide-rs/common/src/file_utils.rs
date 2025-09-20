use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a unique filename for slides
pub fn generate_slide_filename(title: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let slug = create_slug(title);
    format!("{}_{}.md", timestamp, slug)
}

/// Create a URL-friendly slug from a title
pub fn create_slug(title: &str) -> String {
    title
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else if c.is_whitespace() || c == '-' || c == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(50) // Limit length
        .collect()
}

/// Ensure the slides directory exists
pub async fn ensure_slides_dir<P: AsRef<Path>>(slides_dir: P) -> Result<()> {
    let path = slides_dir.as_ref();
    if !path.exists() {
        tokio::fs::create_dir_all(path).await?;
    }
    Ok(())
}

/// Save slide content to file
pub async fn save_slide<P: AsRef<Path>>(
    slides_dir: P,
    filename: &str,
    content: &str,
) -> Result<PathBuf> {
    let slides_dir = slides_dir.as_ref();
    ensure_slides_dir(slides_dir).await?;

    let file_path = slides_dir.join(filename);
    tokio::fs::write(&file_path, content).await?;

    Ok(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_slug() {
        assert_eq!(create_slug("Hello World"), "hello-world");
        assert_eq!(
            create_slug("Project Proposal 2025"),
            "project-proposal-2025"
        );
        assert_eq!(
            create_slug("Test!@# Multiple   Spaces"),
            "test-multiple-spaces"
        );
        assert_eq!(create_slug("日本語タイトル"), "");
    }

    #[test]
    fn test_generate_slide_filename() {
        let filename = generate_slide_filename("Test Presentation");
        assert!(filename.contains("test-presentation"));
        assert!(filename.ends_with(".md"));
    }
}
