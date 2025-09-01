use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct UserNotification {
    pub message: String,
    pub notification_type: String,
    pub timestamp: u64,
}

impl UserNotification {
    pub fn new(message: String, notification_type: String) -> Self {
        Self {
            message,
            notification_type,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    pub fn info(message: String) -> Self {
        Self::new(message, "info".to_string())
    }

    pub fn warning(message: String) -> Self {
        Self::new(message, "warning".to_string())
    }

    pub fn error(message: String) -> Self {
        Self::new(message, "error".to_string())
    }
}

pub fn notify(_json_payload: &str) {
    // Stub: in real system, spawn external notifier.
}

