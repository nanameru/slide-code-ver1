use anyhow::{anyhow, Context, Result};
use clap::{ArgEnum, Parser};
use path_absolutize::Absolutize;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, ArgEnum)]
#[clap(rename_all = "kebab-case")]
enum Mode {
    Create,
    Overwrite,
    Append,
}

#[derive(Debug, Parser)]
#[command(version, about = "Create or edit files under slides/ safely")] 
struct Cli {
    /// Path relative to the repository root (must be under slides/)
    #[arg(long = "path", value_name = "RELATIVE_PATH")]
    path: String,

    /// Operation mode: create (error if exists), overwrite (replace), append (append to end)
    #[arg(long = "mode", value_enum, default_value_t = Mode::Create)]
    mode: Mode,

    /// File content. If omitted, read from stdin.
    #[arg(long = "content")]
    content: Option<String>,

    /// Ensure parent directories exist (create as needed)
    #[arg(long = "ensure-dir", default_value_t = true)]
    ensure_dir: bool,
}

fn ensure_slides_path(root: &Path, rel: &str) -> Result<PathBuf> {
    let p = Path::new(rel);
    if p.is_absolute() {
        return Err(anyhow!("path must be relative to workspace root"));
    }
    let full = root.join(p);
    let abs = full.absolutize().map_err(|e| anyhow!(e.to_string()))?;
    let abs = PathBuf::from(abs.as_ref());
    let slides_root = root.join("slides").absolutize().map_err(|e| anyhow!(e.to_string()))?;
    let slides_root = PathBuf::from(slides_root.as_ref());
    if !abs.starts_with(&slides_root) {
        return Err(anyhow!("path must be under slides/"));
    }
    // Restrict to common slide formats
    let allowed = [".md", ".markdown", ".html", ".htm"]; 
    if let Some(ext) = abs.extension().and_then(|s| s.to_str()) {
        let dot_ext = format!(".{}", ext.to_lowercase());
        if !allowed.iter().any(|a| a.eq_ignore_ascii_case(&dot_ext)) {
            return Err(anyhow!("unsupported extension: allowed: .md, .markdown, .html, .htm"));
        }
    } else {
        return Err(anyhow!("file must have an extension (.md/.html)"));
    }
    Ok(abs)
}

fn read_all_stdin() -> Result<String> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let target = ensure_slides_path(&cwd, &cli.path)?;

    let mut content = if let Some(c) = cli.content { c } else { read_all_stdin()? };
    // Normalize line endings
    if !content.ends_with('\n') { content.push('\n'); }

    if cli.ensure_dir {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("failed to create dir: {}", parent.display()))?;
        }
    }

    match cli.mode {
        Mode::Create => {
            if target.exists() {
                return Err(anyhow!("file already exists: {}", target.display()));
            }
            fs::write(&target, content).with_context(|| format!("failed to write {}", target.display()))?;
        }
        Mode::Overwrite => {
            fs::write(&target, content).with_context(|| format!("failed to write {}", target.display()))?;
        }
        Mode::Append => {
            let mut f = fs::OpenOptions::new().create(true).append(true).open(&target)
                .with_context(|| format!("failed to open {}", target.display()))?;
            f.write_all(content.as_bytes())?;
            f.flush()?;
        }
    }

    println!("[slides_write] ok: {}", target.display());
    Ok(())
}

