use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signal {
    pub protocol_version: u32,
    pub message_id: Uuid,
    pub timestamp: String,
    pub session_id: Option<Uuid>,
    #[serde(rename = "type")]
    pub kind: String,
    pub payload: Value,
}
impl Signal {
    pub fn new(kind: &str, session_id: Option<Uuid>, payload: Value) -> Self {
        Self {
            protocol_version: 1,
            message_id: Uuid::new_v4(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            session_id,
            kind: kind.into(),
            payload,
        }
    }
}
