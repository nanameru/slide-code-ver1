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
                    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Slide Logs</title><style>:root {{ --bg:#f6f5f4; --card:#ffffff; --surface:#fbfaf9; --text:#2f3437; --muted:#6b6f76; --border:#e6e6e6; --accent:#1f7aec; --shadow:0 1px 3px rgba(15,23,42,0.05), 0 8px 30px rgba(15,23,42,0.06); }}@media (prefers-color-scheme: dark) {{ :root {{ --bg:#0f1113; --card:#171a1c; --surface:#14171a; --text:#e6e6e6; --muted:#9aa1a8; --border:#25292d; --accent:#4da3ff; --shadow:0 1px 3px rgba(0,0,0,0.3), 0 8px 30px rgba(0,0,0,0.4); }} }}*{{ box-sizing:border-box; }}body{{ margin:0; background:var(--bg); color:var(--text); font:14px/1.7 -apple-system,BlinkMacSystemFont,Segoe UI,Inter,Helvetica,Arial,Apple Color Emoji,Segoe UI Emoji; }}.top{{ position:sticky; top:0; backdrop-filter:blur(8px); background:color-mix(in oklab, var(--bg) 88%, transparent); border-bottom:1px solid var(--border); }}.top-inner{{ max-width:980px; margin:0 auto; padding:14px 20px; display:flex; align-items:center; gap:12px; }}.crumbs{{ font-size:12px; color:var(--muted); display:flex; gap:6px; align-items:center; }}.crumbs span{{ color:var(--text); }}.pill{{ font-size:12px; color:#fff; background:var(--accent); padding:2px 8px; border-radius:999px; }}.page{{ max-width:980px; margin:28px auto 48px; padding:0 20px; }}.cover{{ background:linear-gradient(180deg, color-mix(in oklab, var(--accent) 18%, transparent), transparent); height:88px; border:1px solid var(--border); border-radius:12px; box-shadow:var(--shadow); }}.page-header{{ margin-top:-44px; padding:0 16px; display:flex; align-items:flex-end; gap:12px; }}.icon{{ width:56px; height:56px; border-radius:12px; display:grid; place-items:center; background:var(--card); border:1px solid var(--border); box-shadow:var(--shadow); font-weight:700; color:var(--accent); }}.title{{ font-size:28px; font-weight:700; letter-spacing:-0.02em; }}.meta{{ font-size:12px; color:var(--muted); margin-top:4px; }}.card{{ background:var(--card); border:1px solid var(--border); border-radius:12px; box-shadow:var(--shadow); overflow:hidden; margin-top:16px; }}.card-head{{ padding:14px 16px; border-bottom:1px solid var(--border); display:flex; align-items:center; justify-content:space-between; gap:12px; flex-wrap:wrap; }}.seg{{ display:flex; gap:6px; background:var(--surface); border:1px solid var(--border); padding:4px; border-radius:10px; }}.seg button{{ background:transparent; color:var(--muted); border:none; padding:6px 10px; border-radius:6px; font:inherit; cursor:pointer; }}.seg button.active{{ background:var(--card); color:var(--text); border:1px solid var(--border); }}.btn{{ background:transparent; color:var(--muted); border:1px solid var(--border); padding:6px 10px; border-radius:8px; font:inherit; cursor:pointer; transition:background .15s ease; }}.btn:hover{{ background:var(--surface); }} .log-wrap{{ background:var(--surface); padding:0; }}.log{{ margin:0; padding:18px 20px; white-space:pre-wrap; overflow:auto; max-height:70vh; background:transparent; font-family:ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace; font-size:12.75px; line-height:1.7; }}.toolbar{{ display:flex; align-items:center; gap:10px; color:var(--muted); font-size:12px; }}.footer{{ text-align:center; color:var(--muted); font-size:12px; margin-top:16px; }}.kbd{{ border:1px solid var(--border); border-bottom-width:2px; background:var(--surface); padding:1px 6px; border-radius:6px; font-size:12px; }}</style><script>function reloadSoon(ms){{ setTimeout(()=>location.reload(), ms); }}function scrollToBottom(){{ const el=document.getElementById('log'); if(el) el.scrollTop=el.scrollHeight; }}async function copyLogs(){{ try{{ const el=document.getElementById('log'); const btn=document.getElementById('copyBtn'); const txt=el?el.innerText:''; await navigator.clipboard.writeText(txt); if(btn){{ const old=btn.innerText; btn.innerText='Copied'; setTimeout(()=>btn.innerText=old,1200); }} }}catch(e){{ alert('Copy failed'); }} }}window.addEventListener('load',()=>{{ scrollToBottom(); reloadSoon(2000); }});</script></head><body><div class=\"top\"><div class=\"top-inner\"><div class=\"crumbs\"><span>Slide</span>›<span>Logs</span>›session.log</div><div class=\"pill\">Live</div></div></div><div class=\"page\"><div class=\"cover\"></div><div class=\"page-header\"><div class=\"icon\">SL</div><div><div class=\"title\">Slide Logs</div><div class=\"meta\">Auto-refresh every 2s • session.log</div></div></div><div class=\"card\"><div class=\"card-head\"><div class=\"toolbar\">View:<div class=\"seg\"><button class=\"active\">Raw</button><button disabled>Table</button></div></div><div class=\"toolbar\"><button id=\"copyBtn\" class=\"btn\" onclick=\"copyLogs()\">Copy</button>Tips: <span class=\"kbd\">i</span> insert <span class=\"kbd\">Enter</span> send <span class=\"kbd\">q</span> quit</div></div><div class=\"log-wrap\"><pre id=\"log\" class=\"log\">{}</pre></div></div><div class=\"footer\">Powered by Slide</div></div></body></html>",
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