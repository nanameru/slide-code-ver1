use anyhow::{anyhow, Context, Result};
use path_absolutize::Absolutize;
use slide_apply_patch::{apply_patch, maybe_parse_apply_patch_verified};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

fn ensure_under_slides(root: &Path, path: &Path) -> Result<()> {
    let joined = root.join(path);
    let abs_tmp = joined.absolutize().map_err(|e| anyhow!(e.to_string()))?;
    let abs = PathBuf::from(abs_tmp.as_ref());
    let slides_join = root.join("slides");
    let slides_root_tmp = slides_join.absolutize().map_err(|e| anyhow!(e.to_string()))?;
    let slides_root = PathBuf::from(slides_root_tmp.as_ref());
    if !abs.starts_with(&slides_root) {
        return Err(anyhow!(format!("path outside slides/: {}", path.display())));
    }
    // Allow only markdown/html
    if let Some(ext) = abs.extension().and_then(|s| s.to_str()) {
        let ext = ext.to_ascii_lowercase();
        if !(ext == "md" || ext == "markdown" || ext == "html" || ext == "htm") {
            return Err(anyhow!(format!("unsupported extension for {}", path.display())));
        }
    } else {
        return Err(anyhow!(format!("missing extension for {}", path.display())));
    }
    Ok(())
}

fn read_all_stdin() -> Result<String> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

fn main() -> Result<()> {
    // Read entire apply_patch payload either from arg[1] or stdin
    let args: Vec<String> = std::env::args().collect();
    let payload = if args.len() > 1 { args[1].clone() } else { read_all_stdin()? };
    let cwd = std::env::current_dir()?;

    // Verify parse and gather affected files
    let verified = maybe_parse_apply_patch_verified(&["apply_patch".to_string(), payload.clone()], &cwd);
    let action = match verified {
        slide_apply_patch::MaybeApplyPatchVerified::Body(action) => action,
        slide_apply_patch::MaybeApplyPatchVerified::NotApplyPatch => return Err(anyhow!("not an apply_patch payload")),
        slide_apply_patch::MaybeApplyPatchVerified::CorrectnessError(reason) => return Err(anyhow!(format!("invalid apply_patch payload: {reason}"))),
        slide_apply_patch::MaybeApplyPatchVerified::ShellParseError(reason) => return Err(anyhow!(format!("invalid apply_patch payload: {:?}", reason))),
    };

    // Enforce slides/ policy on all paths
    for (p, _change) in action.changes() {
        ensure_under_slides(&cwd, p)?;
    }

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    apply_patch(&payload, &mut stdout, &mut stderr).context("apply_patch failed")?;

    print!("{}", String::from_utf8_lossy(&stdout));
    eprint!("{}", String::from_utf8_lossy(&stderr));
    Ok(())
}
