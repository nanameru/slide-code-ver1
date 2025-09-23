use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use tiny_http::{Response, Server};
use tracing::{info, warn, error, debug, trace};
use tracing_subscriber::{EnvFilter, prelude::*};
use tracing_appender::non_blocking;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured logging
    init_tracing()?;
    
    println!("Test Log Viewer starting...");
    info!("Test Log Viewer v1.0.0 with tracing support");

    // Start a tiny local log viewer HTTP server in background
    // - serves / to show tail of /tmp/slide.log
    info!("Starting HTTP log viewer server on port 6060");
    thread::spawn(|| {
        let server = match Server::http("127.0.0.1:6060") {
            Ok(s) => {
                info!("HTTP log viewer server successfully started on 127.0.0.1:6060");
                s
            },
            Err(e) => {
                warn!("Failed to start HTTP server on port 6060: {:?}", e);
                return; // port in use; skip
            }
        };
        // Record the initial size of the log file so we only show logs from this run.
        // If the file does not exist yet, lazily set the offset when it first appears.
        let mut initial_len: Option<u64> =
            std::fs::metadata("/tmp/slide.log").ok().map(|m| m.len());
        loop {
            if let Ok(Some(req)) = server.recv_timeout(Duration::from_millis(200)) {
                // Handle POST request to /clear
                if req.method() == &tiny_http::Method::Post && req.url() == "/clear" {
                    debug!("Received log clear request");
                    // Clear the log file by truncating it
                    let clear_result = std::fs::OpenOptions::new()
                        .write(true)
                        .truncate(true)
                        .create(true)
                        .open("/tmp/slide.log");
                    
                    let response = match clear_result {
                        Ok(_) => {
                            info!("Successfully cleared log file");
                            // Reset initial_len to 0 since we cleared the file
                            initial_len = Some(0);
                            tiny_http::Response::from_string("OK")
                                .with_status_code(200)
                        }
                        Err(e) => {
                            error!("Failed to clear log file: {:?}", e);
                            tiny_http::Response::from_string("Failed to clear log")
                                .with_status_code(500)
                        }
                    };
                    let _ = req.respond(response);
                    continue;
                }

                // Handle GET request to / (default log view)
                if req.method() != &tiny_http::Method::Get || req.url() != "/" {
                    debug!("Received unsupported request: {} {}", req.method(), req.url());
                    let _ = req.respond(
                        tiny_http::Response::from_string("Not Found")
                            .with_status_code(404)
                    );
                    continue;
                }

                trace!("Serving log viewer request");

                // Refresh initial_len lazily and handle truncation/rotation
                if initial_len.is_none() {
                    if let Ok(meta) = std::fs::metadata("/tmp/slide.log") {
                        initial_len = Some(meta.len());
                    }
                }
                if let (Some(offset), Ok(meta)) = (initial_len, std::fs::metadata("/tmp/slide.log"))
                {
                    if meta.len() < offset {
                        // File truncated/rotated; reset baseline to current size
                        initial_len = Some(meta.len());
                    }
                }

                let body = match std::fs::read("/tmp/slide.log") {
                    Ok(bytes) => {
                        let start_u64 = initial_len.unwrap_or(bytes.len() as u64);
                        if (bytes.len() as u64) > start_u64 {
                            let start = start_u64 as usize;
                            String::from_utf8_lossy(&bytes[start..]).to_string()
                        } else {
                            "".to_string()
                        }
                    }
                    Err(_) => "(no log yet)".to_string(),
                };
                let html = format!(
                    "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Test Slide Logs</title><style>:root {{ --bg:#f6f5f4; --card:#ffffff; --surface:#fbfaf9; --text:#2f3437; --muted:#6b6f76; --border:#e6e6e6; --accent:#1f7aec; --shadow:0 1px 3px rgba(15,23,42,0.05), 0 8px 30px rgba(15,23,42,0.06); }}@media (prefers-color-scheme: dark) {{ :root {{ --bg:#0f1113; --card:#171a1c; --surface:#14171a; --text:#e6e6e6; --muted:#9aa1a8; --border:#25292d; --accent:#4da3ff; --shadow:0 1px 3px rgba(0,0,0,0.3), 0 8px 30px rgba(0,0,0,0.4); }} }}*{{ box-sizing:border-box; }}body{{ margin:0; background:var(--bg); color:var(--text); font:14px/1.7 -apple-system,BlinkMacSystemFont,Segoe UI,Inter,Helvetica,Arial,Apple Color Emoji,Segoe UI Emoji; }}.top{{ position:sticky; top:0; backdrop-filter:blur(8px); background:color-mix(in oklab, var(--bg) 88%, transparent); border-bottom:1px solid var(--border); }}.top-inner{{ max-width:980px; margin:0 auto; padding:14px 20px; display:flex; align-items:center; gap:12px; }}.crumbs{{ font-size:12px; color:var(--muted); display:flex; gap:6px; align-items:center; }}.crumbs span{{ color:var(--text); }}.pill{{ font-size:12px; color:#fff; background:var(--accent); padding:2px 8px; border-radius:999px; }}.page{{ max-width:980px; margin:28px auto 48px; padding:0 20px; }}.cover{{ background:linear-gradient(180deg, color-mix(in oklab, var(--accent) 18%, transparent), transparent); height:88px; border:1px solid var(--border); border-radius:12px; box-shadow:var(--shadow); }}.page-header{{ margin-top:-44px; padding:0 16px; display:flex; align-items:flex-end; gap:12px; }}.icon{{ width:56px; height:56px; border-radius:12px; display:grid; place-items:center; background:var(--card); border:1px solid var(--border); box-shadow:var(--shadow); font-weight:700; color:var(--accent); }}.title{{ font-size:28px; font-weight:700; letter-spacing:-0.02em; }}.meta{{ font-size:12px; color:var(--muted); margin-top:4px; }}.card{{ background:var(--card); border:1px solid var(--border); border-radius:12px; box-shadow:var(--shadow); overflow:hidden; margin-top:16px; }}.card-head{{ padding:14px 16px; border-bottom:1px solid var(--border); display:flex; align-items:center; justify-content:space-between; gap:12px; flex-wrap:wrap; }}.seg{{ display:flex; gap:6px; background:var(--surface); border:1px solid var(--border); padding:4px; border-radius:10px; }}.seg button{{ background:transparent; color:var(--muted); border:none; padding:6px 10px; border-radius:6px; font:inherit; cursor:pointer; }}.seg button.active{{ background:var(--card); color:var(--text); border:1px solid var(--border); }}.btn{{ background:transparent; color:var(--muted); border:1px solid var(--border); padding:6px 10px; border-radius:8px; font:inherit; cursor:pointer; transition:background .15s ease; }}.btn:hover{{ background:var(--surface); }} .btn.danger{{ background:transparent; color:#dc2626; border:1px solid #dc2626; }}.btn.danger:hover{{ background:#fef2f2; color:#991b1b; }}.log-wrap{{ background:var(--surface); padding:0; }}.log{{ margin:0; padding:18px 20px; white-space:pre-wrap; overflow:auto; max-height:70vh; background:transparent; font-family:ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace; font-size:12.75px; line-height:1.7; }}.toolbar{{ display:flex; align-items:center; gap:10px; color:var(--muted); font-size:12px; }}.footer{{ text-align:center; color:var(--muted); font-size:12px; margin-top:16px; }}.kbd{{ border:1px solid var(--border); border-bottom-width:2px; background:var(--surface); padding:1px 6px; border-radius:6px; font-size:12px; }}</style><script>function scrollToBottom(){{ const el=document.getElementById('log'); if(el) el.scrollTop=el.scrollHeight; }}async function copyLogs(){{ try{{ const el=document.getElementById('log'); const btn=document.getElementById('copyBtn'); const txt=el?el.innerText:''; await navigator.clipboard.writeText(txt); if(btn){{ const old=btn.innerText; btn.innerText='Copied'; setTimeout(()=>btn.innerText=old,1200); }} }}catch(e){{ alert('Copy failed'); }} }}async function clearLogs(){{ const confirmClear = confirm('ログをクリアしてもよろしいですか？'); if(!confirmClear) return; try{{ const response = await fetch('/clear', {{ method: 'POST' }}); const btn = document.getElementById('clearBtn'); if(response.ok){{ const old = btn.innerText; btn.innerText = 'Cleared'; setTimeout(() => {{ btn.innerText = old; location.reload(); }}, 1000); }}else{{ alert('ログのクリアに失敗しました'); }} }}catch(e){{ alert('ログのクリアに失敗しました'); }} }}(function(){{ try{{ if('scrollRestoration' in history){{ history.scrollRestoration='manual'; }} }}catch(e){{}} function saveScroll(){{ const el=document.getElementById('log'); if(el) sessionStorage.setItem('logScrollTop', String(el.scrollTop)); }} function restoreScroll(){{ const el=document.getElementById('log'); if(!el) return; const v=sessionStorage.getItem('logScrollTop'); if(v!==null){{ const n=parseInt(v,10); if(!Number.isNaN(n)) el.scrollTop=n; }} }} window.addEventListener('beforeunload', saveScroll); window.addEventListener('load',()=>{{ restoreScroll(); }}); }})();</script></head><body><div class=\"top\"><div class=\"top-inner\"><div class=\"crumbs\"><span>Test Slide</span>›<span>Logs</span>›session.log</div><div class=\"pill\">Live</div></div></div><div class=\"page\"><div class=\"cover\"></div><div class=\"page-header\"><div class=\"icon\">TSL</div><div><div class=\"title\">Test Slide Logs</div><div class=\"meta\">Manual refresh • session.log</div></div></div><div class=\"card\"><div class=\"card-head\"><div class=\"toolbar\">View:<div class=\"seg\"><button class=\"active\">Raw</button><button disabled>Table</button></div></div><div class=\"toolbar\"><button id=\"reloadBtn\" class=\"btn\" onclick=\"location.reload()\">Reload</button><button id=\"copyBtn\" class=\"btn\" onclick=\"copyLogs()\">Copy</button><button id=\"clearBtn\" class=\"btn danger\" onclick=\"clearLogs()\">Clear</button>Tips: <span class=\"kbd\">Test</span> version</div></div><div class=\"log-wrap\"><pre id=\"log\" class=\"log\">{}</pre></div></div><div class=\"footer\">Powered by Test Slide</div></div></body></html>",
                    html_escape::encode_text(&body)
                );
                let _ = req.respond(
                    Response::from_string(html)
                        .with_header(
                            tiny_http::Header::from_bytes(
                                &b"Content-Type"[..],
                                &b"text/html; charset=utf-8"[..],
                            )
                            .unwrap(),
                        )
                        .with_header(
                            tiny_http::Header::from_bytes(
                                &b"Cache-Control"[..],
                                &b"no-store, must-revalidate"[..],
                            )
                            .unwrap(),
                        )
                        .with_header(
                            tiny_http::Header::from_bytes(&b"Pragma"[..], &b"no-cache"[..])
                                .unwrap(),
                        )
                        .with_header(
                            tiny_http::Header::from_bytes(&b"Expires"[..], &b"0"[..]).unwrap(),
                        ),
                );
            } else {
                // idle
            }
        }
    });

    // Open browser to log page (best-effort)
    info!("Opening browser to log viewer at http://127.0.0.1:6060/");
    if let Err(e) = webbrowser::open("http://127.0.0.1:6060/") {
        warn!("Failed to open browser: {:?}", e);
    }

    // Generate some test logs
    info!("Generating test log messages");
    for i in 0..10 {
        info!("Test log message #{}: This is a sample log entry", i + 1);
        debug!("Debug message #{}: Detailed information for debugging", i + 1);
        warn!("Warning message #{}: This is a warning message", i + 1);
        thread::sleep(Duration::from_millis(500));
    }

    println!("Test log viewer running. Press Ctrl+C to stop.");
    println!("Visit http://127.0.0.1:6060/ to view logs.");
    
    // Keep the main thread alive
    loop {
        thread::sleep(Duration::from_secs(5));
        info!("Heartbeat: Server is still running");
    }
}

fn init_tracing() -> Result<(), Box<dyn std::error::Error>> {
    // Create a file appender that writes to /tmp/slide.log
    let file_appender = tracing_appender::rolling::never("/tmp", "slide.log");
    let (non_blocking_file, _guard) = non_blocking(file_appender);
    
    // Set up the logging configuration
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_file)
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .with_file(true)
        .with_ansi(false); // Disable ANSI codes for file output
    
    // Create console layer for debugging
    let console_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true)
        .with_file(true);
    
    // Set up environment filter (can be controlled via RUST_LOG env var)
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("test_log_viewer=debug,info"));
    
    // Initialize the registry with both layers
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(console_layer)
        .init();
    
    // Ensure the guard is not dropped immediately
    std::mem::forget(_guard);
    
    info!("Tracing initialized successfully");
    Ok(())
}
