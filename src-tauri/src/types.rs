use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct LiveState {
    pub is_running: bool,
    pub started_at: Option<DateTime<Utc>>,
}
