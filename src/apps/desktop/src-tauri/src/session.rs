// Manages AI generation state and session handling

use tokio_util::sync::CancellationToken;

pub struct GenerationState {
    pub is_running: bool,
    pub current_session_id: Option<i64>,
    pub cancellation_token: Option<CancellationToken>,
}

impl Default for GenerationState {
    fn default() -> Self {
        Self {
            is_running: false,
            current_session_id: Some(-1),
            cancellation_token: None,
        }
    }
}
