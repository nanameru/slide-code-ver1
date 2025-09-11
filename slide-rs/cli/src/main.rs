use slide_arg0::arg0_dispatch_or_else;
use slide_tui::Cli as TuiCli;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use tiny_http::{Server, Response};
use webbrowser;

fn main() -> anyhow::Result<()> {
    // Check if we're in Slide mode via environment variable
    let is_slide_mode = std::env::var("SLIDE_APP").is_ok();
    // Load env.local if present (OPENAI_API_KEY etc.)
    try_load_env_local();
    
    arg0_dispatch_or_else(|slide_linux_sandbox_exe| async move {
        cli_main(slide_linux_sandbox_exe, is_slide_mode).await?;
        Ok(())
    })
}

async fn cli_main(slide_linux_sandbox_exe: Option<PathBuf>, is_slide_mode: bool) -> anyhow::Result<()> {
    println!("Slide CLI v0.0.1");
    
    if is_slide_mode {
        println!("Running in Slide mode");
    }
    
    // Start a tiny local log viewer HTTP server in background
    // - serves / to show tail of /tmp/slide.log
    thread::spawn(|| {
        let server = match Server::http("127.0.0.1:6060") {
            Ok(s) => s,
            Err(_) => return, // port in use; skip
        };
        loop {
            if let Ok(Some(req)) = server.recv_timeout(Duration::from_millis(200)) {
                let body = std::fs::read_to_string("/tmp/slide.log").unwrap_or_else(|_| "(no log yet)".to_string());
                let html = format!(
                    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Slide Logs</title><style>:root {{ --bg:#f6f5f4; --card:#ffffff; --text:#2f3437; --muted:#6b6f76; --border:#e6e6e6; --accent:#1f7aec; --shadow:0 1px 3px rgba(15,23,42,0.06), 0 6px 24px rgba(15,23,42,0.04); }}*{{ box-sizing:border-box; }}body{{ margin:0; background:var(--bg); color:var(--text); font:14px/1.6 -apple-system,BlinkMacSystemFont,Segoe UI,Inter,Helvetica,Arial,Apple Color Emoji,Segoe UI Emoji; }}.header{{ position:sticky; top:0; backdrop-filter:blur(6px); background:rgba(246,245,244,0.85); border-bottom:1px solid var(--border); }}.header-inner{{ max-width:960px; margin:0 auto; padding:12px 20px; display:flex; align-items:center; gap:12px; }}.badge{{ font-size:12px; color:#fff; background:var(--accent); padding:2px 8px; border-radius:999px; }}.container{{ max-width:960px; margin:20px auto; padding:0 20px 40px; }}.card{{ background:var(--card); border:1px solid var(--border); border-radius:12px; box-shadow:var(--shadow); overflow:hidden; }}.card-header{{ padding:16px 18px; border-bottom:1px solid var(--border); display:flex; align-items:center; justify-content:space-between; }}.title{{ font-weight:600; }}.hint{{ font-size:12px; color:var(--muted); }}.log{{ margin:0; padding:16px 18px; white-space:pre-wrap; background:#fbfbfa; font-family:ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace; font-size:12.5px; line-height:1.6; }}.footer{{ text-align:center; color:var(--muted); font-size:12px; margin-top:16px; }}</style><script>window.addEventListener('load',()=>{{ const el=document.getElementById('log'); if(el){{ el.scrollTop=el.scrollHeight; }} setTimeout(()=>location.reload(),2000); }});</script></head><body><div class=\"header\"><div class=\"header-inner\"><div class=\"badge\">Live</div><div>Slide Logs</div></div></div><div class=\"container\"><div class=\"card\"><div class=\"card-header\"><div class=\"title\">session.log</div><div class=\"hint\">auto refresh: 2s</div></div><pre id=\"log\" class=\"log\">{}</pre></div><div class=\"footer\">Hints: i=insert • Enter=send • q=quit • h=help</div></div></body></html>",
                    html_escape::encode_text(&body)
                );
                let _ = req.respond(Response::from_string(html).with_header(
                    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()
                ));
            } else {
                // idle
            }
        }
    });

    // Open browser to log page (best-effort)
    let _ = webbrowser::open("http://127.0.0.1:6060/");

    // For now, just run the TUI
    let tui_cli = TuiCli::default();
    slide_tui::run_main(tui_cli, slide_linux_sandbox_exe).await?;
    
    Ok(())
}

fn try_load_env_local() {
    if std::env::var("OPENAI_API_KEY").is_ok() {
        return;
    }
    let candidates = [
        std::env::current_dir().ok(),
        std::env::current_dir().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())),
        std::env::current_dir().ok().and_then(|p| p.parent().and_then(|q| q.parent()).map(|p| p.to_path_buf())),
    ];
    for base in candidates.into_iter().flatten() {
        for name in ["env.local", ".env.local"] {
            let path = base.join(name);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for line in content.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') { continue; }
                        if let Some((k,v)) = parse_env_line(line) {
                            if std::env::var(&k).is_err() {
                                std::env::set_var(k, v);
                            }
                        }
                    }
                }
                return;
            }
        }
    }
}

fn parse_env_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.splitn(2, '=');
    let key = parts.next()?.trim();
    let val_raw = parts.next()?.trim();
    let val = val_raw.trim_matches('"').trim_matches('\'');
    if key.is_empty() { return None; }
    Some((key.to_string(), val.to_string()))
}