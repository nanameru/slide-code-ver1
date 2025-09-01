use std::collections::HashMap;
use super::{ExecCommandSession, SessionId};

#[derive(Default)]
pub struct SessionManager { sessions: HashMap<String, ExecCommandSession> }

impl SessionManager {
    pub fn new() -> Self { Self::default() }
    pub fn create(&mut self, id: String) -> SessionId {
        let s = ExecCommandSession { id: id.clone() };
        self.sessions.insert(id.clone(), s);
        SessionId(id)
    }
}

