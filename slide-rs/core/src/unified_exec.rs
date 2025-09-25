//! Unified execution system for interactive shell sessions.
//!
//! This module provides a unified execution system that maintains persistent
//! shell sessions across multiple commands, based on codex-1's implementation.

use std::collections::{HashMap, VecDeque};
use std::io::{ErrorKind, Read};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use tokio::sync::{Mutex, Notify, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use anyhow::{anyhow, Result};
use serde::Deserialize;

// use crate::exec_command::ExecCommandSession; // Commented out for now
use crate::conversation_history::{FunctionCallOutputPayload, ResponseInputItem};

/// Dummy child implementation for cases where we've moved the real child
struct DummyChild;

impl std::fmt::Debug for DummyChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DummyChild")
    }
}

impl portable_pty::ChildKiller for DummyChild {
    fn kill(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
        Box::new(DummyChild)
    }
}

impl portable_pty::Child for DummyChild {
    fn try_wait(&mut self) -> std::io::Result<Option<portable_pty::ExitStatus>> {
        Ok(Some(portable_pty::ExitStatus::with_exit_code(0)))
    }

    fn wait(&mut self) -> std::io::Result<portable_pty::ExitStatus> {
        Ok(portable_pty::ExitStatus::with_exit_code(0))
    }

    fn process_id(&self) -> Option<u32> {
        None
    }
}

/// Simplified ExecCommandSession for unified_exec
#[derive(Debug)]
pub struct ExecCommandSession {
    writer_sender: mpsc::Sender<Vec<u8>>,
    killer: Box<dyn portable_pty::Child + Send + Sync>,
    reader_handle: JoinHandle<()>,
    writer_handle: JoinHandle<()>,
    wait_handle: JoinHandle<()>,
    exit_status: Arc<AtomicBool>,
}

impl ExecCommandSession {
    pub fn new(
        writer_sender: mpsc::Sender<Vec<u8>>,
        _output_sender: tokio::sync::broadcast::Sender<Vec<u8>>,
        killer: Box<dyn portable_pty::Child + Send + Sync>,
        reader_handle: JoinHandle<()>,
        writer_handle: JoinHandle<()>,
        wait_handle: JoinHandle<()>,
        exit_status: Arc<AtomicBool>,
    ) -> (Self, tokio::sync::broadcast::Receiver<Vec<u8>>) {
        let output_receiver = _output_sender.subscribe();
        let session = Self {
            writer_sender,
            killer,
            reader_handle,
            writer_handle,
            wait_handle,
            exit_status,
        };
        (session, output_receiver)
    }
    
    pub fn writer_sender(&self) -> mpsc::Sender<Vec<u8>> {
        self.writer_sender.clone()
    }
    
    pub fn has_exited(&self) -> bool {
        self.exit_status.load(Ordering::SeqCst)
    }
}

impl Drop for ExecCommandSession {
    fn drop(&mut self) {
        self.reader_handle.abort();
        self.writer_handle.abort();
        self.wait_handle.abort();
    }
}

const DEFAULT_TIMEOUT_MS: u64 = 1_000;
const MAX_TIMEOUT_MS: u64 = 60_000;
const UNIFIED_EXEC_OUTPUT_MAX_BYTES: usize = 128 * 1024; // 128 KiB

/// Arguments for unified_exec tool calls
#[derive(Debug, Deserialize)]
pub struct UnifiedExecArgs {
    pub input: Vec<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Request for unified execution
#[derive(Debug)]
pub struct UnifiedExecRequest<'a> {
    pub session_id: Option<i32>,
    pub input_chunks: &'a [String],
    pub timeout_ms: Option<u64>,
}

/// Result from unified execution
#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedExecResult {
    pub session_id: Option<i32>,
    pub output: String,
}

/// Error types for unified execution
#[derive(Debug, thiserror::Error)]
pub enum UnifiedExecError {
    #[error("Unknown session ID: {session_id}")]
    UnknownSessionId { session_id: i32 },
    #[error("Missing command line")]
    MissingCommandLine,
    #[error("Failed to create session: {0}")]
    CreateSession(#[from] anyhow::Error),
    #[error("Failed to write to stdin")]
    WriteToStdin,
}

/// Session manager for unified execution
#[derive(Debug, Default)]
pub struct UnifiedExecSessionManager {
    next_session_id: AtomicI32,
    sessions: Mutex<HashMap<i32, ManagedUnifiedExecSession>>,
}

/// Managed unified execution session
#[derive(Debug)]
struct ManagedUnifiedExecSession {
    session: ExecCommandSession,
    output_buffer: OutputBuffer,
    output_notify: Arc<Notify>,
    output_task: JoinHandle<()>,
}

/// Output buffer state
#[derive(Debug, Default)]
struct OutputBufferState {
    chunks: VecDeque<Vec<u8>>,
    total_bytes: usize,
}

impl OutputBufferState {
    fn push_chunk(&mut self, chunk: Vec<u8>) {
        self.total_bytes = self.total_bytes.saturating_add(chunk.len());
        self.chunks.push_back(chunk);

        let mut excess = self
            .total_bytes
            .saturating_sub(UNIFIED_EXEC_OUTPUT_MAX_BYTES);

        while excess > 0 {
            match self.chunks.front_mut() {
                Some(front) if excess >= front.len() => {
                    excess -= front.len();
                    self.total_bytes = self.total_bytes.saturating_sub(front.len());
                    self.chunks.pop_front();
                }
                Some(front) => {
                    front.drain(..excess);
                    self.total_bytes = self.total_bytes.saturating_sub(excess);
                    break;
                }
                None => break,
            }
        }
    }

    fn drain(&mut self) -> Vec<Vec<u8>> {
        let drained: Vec<Vec<u8>> = self.chunks.drain(..).collect();
        self.total_bytes = 0;
        drained
    }
}

type OutputBuffer = Arc<Mutex<OutputBufferState>>;
type OutputHandles = (OutputBuffer, Arc<Notify>);

impl ManagedUnifiedExecSession {
    fn new(
        session: ExecCommandSession,
        initial_output_rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    ) -> Self {
        let output_buffer = Arc::new(Mutex::new(OutputBufferState::default()));
        let output_notify = Arc::new(Notify::new());
        let mut receiver = initial_output_rx;
        let buffer_clone = Arc::clone(&output_buffer);
        let notify_clone = Arc::clone(&output_notify);
        let output_task = tokio::spawn(async move {
            while let Ok(chunk) = receiver.recv().await {
                let mut guard = buffer_clone.lock().await;
                guard.push_chunk(chunk);
                drop(guard);
                notify_clone.notify_waiters();
            }
        });

        Self {
            session,
            output_buffer,
            output_notify,
            output_task,
        }
    }

    fn writer_sender(&self) -> mpsc::Sender<Vec<u8>> {
        self.session.writer_sender()
    }

    fn output_handles(&self) -> OutputHandles {
        (
            Arc::clone(&self.output_buffer),
            Arc::clone(&self.output_notify),
        )
    }

    fn has_exited(&self) -> bool {
        self.session.has_exited()
    }
}

impl Drop for ManagedUnifiedExecSession {
    fn drop(&mut self) {
        self.output_task.abort();
    }
}

impl UnifiedExecSessionManager {
    pub async fn handle_request(
        &self,
        request: UnifiedExecRequest<'_>,
    ) -> Result<UnifiedExecResult, UnifiedExecError> {
        let (timeout_ms, timeout_warning) = match request.timeout_ms {
            Some(requested) if requested > MAX_TIMEOUT_MS => (
                MAX_TIMEOUT_MS,
                Some(format!(
                    "Warning: requested timeout {requested}ms exceeds maximum of {MAX_TIMEOUT_MS}ms; clamping to {MAX_TIMEOUT_MS}ms.\n"
                )),
            ),
            Some(requested) => (requested, None),
            None => (DEFAULT_TIMEOUT_MS, None),
        };

        let mut new_session: Option<ManagedUnifiedExecSession> = None;
        let session_id;
        let writer_tx;
        let output_buffer;
        let output_notify;

        if let Some(existing_id) = request.session_id {
            let mut sessions = self.sessions.lock().await;
            match sessions.get(&existing_id) {
                Some(session) => {
                    if session.has_exited() {
                        sessions.remove(&existing_id);
                        return Err(UnifiedExecError::UnknownSessionId {
                            session_id: existing_id,
                        });
                    }
                    let (buffer, notify) = session.output_handles();
                    session_id = existing_id;
                    writer_tx = session.writer_sender();
                    output_buffer = buffer;
                    output_notify = notify;
                }
                None => {
                    return Err(UnifiedExecError::UnknownSessionId {
                        session_id: existing_id,
                    });
                }
            }
            drop(sessions);
        } else {
            let command = request.input_chunks.to_vec();
            let new_id = self.next_session_id.fetch_add(1, Ordering::SeqCst);
            let (session, initial_output_rx) = create_unified_exec_session(&command).await?;
            let managed_session = ManagedUnifiedExecSession::new(session, initial_output_rx);
            let (buffer, notify) = managed_session.output_handles();
            writer_tx = managed_session.writer_sender();
            output_buffer = buffer;
            output_notify = notify;
            session_id = new_id;
            new_session = Some(managed_session);
        };

        if request.session_id.is_some() {
            let joined_input = request.input_chunks.join(" ");
            if !joined_input.is_empty() && writer_tx.send(joined_input.into_bytes()).await.is_err()
            {
                return Err(UnifiedExecError::WriteToStdin);
            }
        }

        let mut collected: Vec<u8> = Vec::with_capacity(4096);
        let start = Instant::now();
        let deadline = start + Duration::from_millis(timeout_ms);

        loop {
            let drained_chunks;
            let mut wait_for_output = None;
            {
                let mut guard = output_buffer.lock().await;
                drained_chunks = guard.drain();
                if drained_chunks.is_empty() {
                    wait_for_output = Some(output_notify.notified());
                }
            }

            if drained_chunks.is_empty() {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining == Duration::ZERO {
                    break;
                }

                let notified = wait_for_output.unwrap_or_else(|| output_notify.notified());
                tokio::pin!(notified);
                tokio::select! {
                    _ = &mut notified => {}
                    _ = tokio::time::sleep(remaining) => break,
                }
                continue;
            }

            for chunk in drained_chunks {
                collected.extend_from_slice(&chunk);
            }

            if Instant::now() >= deadline {
                break;
            }
        }

        let output = String::from_utf8_lossy(&collected);
        let output = if let Some(warning) = timeout_warning {
            format!("{warning}{output}")
        } else {
            output.to_string()
        };

        let should_store_session = if let Some(session) = new_session.as_ref() {
            !session.has_exited()
        } else if request.session_id.is_some() {
            let mut sessions = self.sessions.lock().await;
            if let Some(existing) = sessions.get(&session_id) {
                if existing.has_exited() {
                    sessions.remove(&session_id);
                    false
                } else {
                    true
                }
            } else {
                false
            }
        } else {
            true
        };

        if should_store_session {
            if let Some(session) = new_session {
                self.sessions.lock().await.insert(session_id, session);
            }
            Ok(UnifiedExecResult {
                session_id: Some(session_id),
                output,
            })
        } else {
            Ok(UnifiedExecResult {
                session_id: None,
                output,
            })
        }
    }
}

/// Create a unified exec session
async fn create_unified_exec_session(
    command: &[String],
) -> Result<
    (
        ExecCommandSession,
        tokio::sync::broadcast::Receiver<Vec<u8>>,
    ),
    UnifiedExecError,
> {
    if command.is_empty() {
        return Err(UnifiedExecError::MissingCommandLine);
    }

    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| UnifiedExecError::CreateSession(anyhow!("Failed to create PTY: {}", e)))?;

    // Safe thanks to the check at the top of the function.
    let mut command_builder = CommandBuilder::new(command[0].clone());
    for arg in &command[1..] {
        command_builder.arg(arg);
    }

    let child = pair
        .slave
        .spawn_command(command_builder)
        .map_err(|e| UnifiedExecError::CreateSession(anyhow!("Failed to spawn command: {}", e)))?;
    let killer = child;

    let (writer_tx, mut writer_rx) = mpsc::channel::<Vec<u8>>(128);
    let (output_tx, _) = tokio::sync::broadcast::channel::<Vec<u8>>(256);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| UnifiedExecError::CreateSession(anyhow!("Failed to clone reader: {}", e)))?;
    let output_tx_clone = output_tx.clone();
    let reader_handle = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = output_tx_clone.send(buf[..n].to_vec());
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(_) => break,
            }
        }
    });

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| UnifiedExecError::CreateSession(anyhow!("Failed to take writer: {}", e)))?;
    let writer = Arc::new(StdMutex::new(writer));
    let writer_handle = tokio::spawn({
        let writer = writer.clone();
        async move {
            while let Some(bytes) = writer_rx.recv().await {
                let writer = writer.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(mut guard) = writer.lock() {
                        use std::io::Write;
                        let _ = guard.write_all(&bytes);
                        let _ = guard.flush();
                    }
                })
                .await;
            }
        }
    });

    let exit_status = Arc::new(AtomicBool::new(false));
    let wait_exit_status = Arc::clone(&exit_status);
    let mut child_for_wait = killer;
    let wait_handle = tokio::task::spawn_blocking(move || {
        let _ = child_for_wait.wait();
        wait_exit_status.store(true, Ordering::SeqCst);
    });
    
    // Create a dummy killer for the session (since we moved the child)
    let dummy_killer: Box<dyn portable_pty::Child + Send + Sync> = Box::new(DummyChild);

    let (session, initial_output_rx) = ExecCommandSession::new(
        writer_tx,
        output_tx,
        dummy_killer,
        reader_handle,
        writer_handle,
        wait_handle,
        exit_status,
    );
    Ok((session, initial_output_rx))
}

/// Handle unified_exec tool call
pub async fn handle_unified_exec_tool_call(
    _sess: &crate::container_exec::Session, // Use forward declaration for now
    call_id: String,
    session_id: Option<String>,
    input: Vec<String>,
    timeout_ms: Option<u64>,
) -> ResponseInputItem {
    // Parse session_id if provided
    let session_id_int = session_id.and_then(|s| s.parse::<i32>().ok());
    
    let request = UnifiedExecRequest {
        session_id: session_id_int,
        input_chunks: &input,
        timeout_ms,
    };
    
    // For now, use a static session manager (in real implementation, this would be part of Session)
    static UNIFIED_EXEC_MANAGER: std::sync::OnceLock<UnifiedExecSessionManager> = std::sync::OnceLock::new();
    let manager = UNIFIED_EXEC_MANAGER.get_or_init(|| UnifiedExecSessionManager::default());
    
    let result = manager.handle_request(request).await;
    
    let output_payload = match result {
        Ok(value) => {
            // Serialize the result
            match serde_json::to_string_pretty(&serde_json::json!({
                "session_id": value.session_id,
                "output": &value.output,
            })) {
                Ok(serialized) => FunctionCallOutputPayload {
                    content: serialized,
                    success: Some(true),
                },
                Err(err) => FunctionCallOutputPayload {
                    content: format!("failed to serialize unified exec output: {err}"),
                    success: Some(false),
                },
            }
        }
        Err(err) => FunctionCallOutputPayload {
            content: format!("unified exec failed: {err}"),
            success: Some(false),
        },
    };

    ResponseInputItem::FunctionCallOutput {
        call_id,
        output: output_payload,
    }
}
