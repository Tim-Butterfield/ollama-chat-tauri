// Handles Tauri command definitions

use crate::db;
use crate::ollama_api;
use crate::session::GenerationState;
use std::sync::Arc;
use tauri::{command, State};
use tokio::sync::Mutex;
use serde_json::Value;
use rusqlite::Connection;


#[command]
pub async fn load_models() -> Result<Vec<String>, String> {
    ollama_api::fetch_models().await
}

// Get selected model
#[command]
pub async fn get_selected_model(conn: tauri::State<'_, Arc<Mutex<Connection>>>) -> Result<String, String> {
    let conn = conn.lock().await;
    db::get_config_value(&conn, "selected_model_name")
        .map(|model| model.unwrap_or_else(|| "".to_string()))
        .map_err(|e| e.to_string())
}

// Save selected model
#[command]
pub async fn save_selected_model(
    conn: tauri::State<'_, Arc<Mutex<Connection>>>,
    model_name: String,
) -> Result<(), String> {
    let conn = conn.lock().await;
    db::update_config_value(&conn, "selected_model_name", &model_name)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[command]
pub async fn delete_chat_session(
    session_id: i64,
    db: State<'_, Arc<Mutex<rusqlite::Connection>>>,
    state: State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<(), String> {
    let result = db::remove_chat_session(session_id, db, state).await;
    result.map_err(|e| format!("Failed to delete session: {}", e))
}

#[command]
pub async fn get_current_session(
    db: State<'_, Arc<Mutex<rusqlite::Connection>>>,
    state: State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<db::CurrentSession, String> {
    db::fetch_current_session(db, state).await.map_err(|e| e.to_string())
}

#[command]
pub async fn update_chat_session_name(
    session_id: i64,
    new_name: String,
    db: State<'_, Arc<Mutex<rusqlite::Connection>>>,
) -> Result<(), String> {
    db::rename_chat_session(session_id, new_name, db).await.map_err(|e| e.to_string())
}

#[command]
pub async fn load_chat_sessions(
    db: State<'_, Arc<Mutex<rusqlite::Connection>>>,
) -> Result<Vec<db::ChatSession>, String> {
    db::fetch_chat_sessions(db).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_current_session(
    state: tauri::State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<(), String> {
    let mut gen_state = state.lock().await;
    gen_state.current_session_id = Some(-1);
    Ok(())
}

#[tauri::command]
pub async fn set_current_session(
    session_id: i64,
    state: tauri::State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<(), String> {
    let mut gen_state = state.lock().await;
    gen_state.current_session_id = Some(session_id);
    Ok(())
}

#[command]
pub async fn generate_chat(
    prompt: String,
    model: String,
    state: State<'_, Arc<Mutex<GenerationState>>>,
    db_conn: State<'_, Arc<Mutex<rusqlite::Connection>>>,
) -> Result<String, String> {
    ollama_api::process_chat_generation(prompt, model, state, db_conn).await
}

#[command]
pub async fn load_chat_history(
    state: State<'_, Arc<Mutex<GenerationState>>>,
    db_conn: State<'_, Arc<Mutex<rusqlite::Connection>>>,
) -> Result<Vec<Value>, String> {

    let session_id = {
        let state_guard = state.lock().await;
        state_guard.current_session_id.unwrap_or(-1)
    };
    
    let chat_messages = db::fetch_chat_history(session_id, db_conn)
    .await
    .map_err(|e| e.to_string())?;

    let json_messages: Vec<Value> = chat_messages
        .into_iter()
        .map(|msg| serde_json::json!({
            "id": msg.id,
            "session_id": msg.session_id,
            "role": msg.role,
            "content": msg.message,
            "timestamp": msg.timestamp
        }))
        .collect();

    Ok(json_messages)
}

// Abort chat generation
#[command]
pub async fn abort_generation(state: tauri::State<'_, Arc<Mutex<GenerationState>>>) -> Result<(), String> {
    let mut generation_state = state.lock().await;

    if let Some(token) = &generation_state.cancellation_token {
        token.cancel(); // Trigger cancellation
    }

    generation_state.is_running = false; // Update state to indicate generation is no longer running
    generation_state.cancellation_token = None; // Clear the cancellation token

    Ok(())
}
