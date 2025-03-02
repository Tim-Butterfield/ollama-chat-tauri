
// Handles SQLite database operations

use crate::session::GenerationState;
use rusqlite::{params, Connection, Result, OptionalExtension};
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::State;
use tauri::api::path::app_data_dir;
use std::fs;
use std::path::PathBuf;


#[derive(Debug, serde::Serialize)]
pub struct ChatSession {
   pub id: i64,
    pub title: String,
}

#[derive(Debug, serde::Serialize)]
pub struct CurrentSession {
    pub id: i64,
    pub title: String,
}

/// Represents a chat message entry.
#[derive(Debug, serde::Serialize)]
pub struct ChatMessage {
    pub id: i64,
    pub session_id: i64,
    pub role: String,
    pub message: String,
    pub timestamp: String,
}


// Initialize SQLite Database
pub fn init_db() -> Arc<Mutex<Connection>> {
    // Get the app data directory for the platform
    let base_dir = app_data_dir(&tauri::Config::default())
        .expect("Failed to retrieve application data directory")
        .join("ollama-chat-tauri");

    let db_path: PathBuf = base_dir.join("ollama-chat-tauri.db");

    // Ensure the directory exists
    if let Some(parent) = db_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).expect("Failed to create database directory");
        }
    }

    let conn = Connection::open(db_path).expect("Failed to open SQLite database");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_config (
            key TEXT PRIMARY KEY,
            value TEXT
        )",
        [],
    ).expect("Failed to create app_config table");

    // Chat sessions table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS chat_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    ).expect("Failed to create chat_sessions table");
    
    // Chat history table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS chat_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id INTEGER NOT NULL,
            role TEXT NOT NULL,
            message TEXT NOT NULL,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (session_id) REFERENCES chat_sessions(id)
        )",
        [],
    ).expect("Failed to create chat_history table");

    Arc::new(Mutex::new(conn))
}

/// Inserts or updates a configuration key-value pair.
pub fn update_config_value(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    //println!("Updating config value: key = {}, value = {}", 
    //    key, value);

    conn.execute(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES (?1, ?2)",
        [key, value],
    )?;
    Ok(())
}

/// Retrieves a configuration value by key. Returns `None` if the key doesn't exist.
pub fn get_config_value(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM app_config WHERE key = ?1",
        [key],
        |row| row.get(0),
    ).optional()
}

pub async fn remove_chat_session(
    session_id: i64,
    db: State<'_, Arc<Mutex<rusqlite::Connection>>>,
    state: State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<(), rusqlite::Error> {
    let conn = db.lock().await;
    conn.execute(
        "DELETE FROM chat_sessions WHERE id = ?1",
        params![session_id],
    )?;

    // Check if the deleted session is the current session
    let mut generation_state = state.lock().await;
    if generation_state.current_session_id == Some(session_id) {
        generation_state.current_session_id = Some(-1); // Clear the current session
    }

    Ok(())
}

pub async fn fetch_current_session(
    db: State<'_, Arc<Mutex<rusqlite::Connection>>>,
    state: State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<CurrentSession, rusqlite::Error> {
    let gen_state = state.lock().await;

    match gen_state.current_session_id {
        Some(id) if id != -1 => {
            let conn = db.lock().await;
            let mut stmt = conn.prepare("SELECT title FROM chat_sessions WHERE id = ?1")?;

            let title: Option<String> = stmt.query_row(params![id], |row| row.get(0)).optional()?;

            Ok(CurrentSession {
                id,
                title: title.unwrap_or_default(),
            })
        }
        _ => Ok(CurrentSession {
            id: -1,
            title: String::new(),
        }),
    }
}

pub async fn rename_chat_session(
    session_id: i64,
    new_name: String,
    db: State<'_, Arc<Mutex<rusqlite::Connection>>>,
) -> Result<(), rusqlite::Error> {
    let conn = db.lock().await;
    conn.execute(
        "UPDATE chat_sessions SET title = ?1 WHERE id = ?2",
        params![new_name, session_id],
    )?;

    Ok(())
}

pub async fn fetch_chat_sessions(
    db: State<'_, Arc<Mutex<rusqlite::Connection>>>,
) -> Result<Vec<ChatSession>, rusqlite::Error> {
    let conn = db.lock().await;
    let mut stmt = conn.prepare("SELECT id, title FROM chat_sessions ORDER BY id DESC")?;
    let sessions_iter = stmt.query_map([], |row| {
        Ok(ChatSession {
            id: row.get(0)?,
            title: row.get(1)?,
        })
    })?;

    let mut sessions = Vec::new();
    for session in sessions_iter {
        sessions.push(session?);
    }

    Ok(sessions)
}

pub async fn get_or_create_session(conn: &Arc<Mutex<Connection>>, title: &str) -> Result<i64, String> {
    let conn = conn.lock().await;

    // Check if a session with the given title exists
    let mut stmt = conn
        .prepare("SELECT id FROM chat_sessions WHERE title = ?1")
        .map_err(|e| e.to_string())?;

    let session_id: Option<i64> = stmt
        .query_row([title], |row| row.get(0))
        .optional()
        .map_err(|e| e.to_string())?;

    // If exists, return the session ID
    if let Some(id) = session_id {
        Ok(id)
    } else {
        // If not, create a new session
        conn.execute(
            "INSERT INTO chat_sessions (title) VALUES (?1)",
            rusqlite::params![title],
        )
        .map_err(|e| e.to_string())?;

        Ok(conn.last_insert_rowid())
    }
}

/// Fetches the chat history for a given session.
pub async fn fetch_chat_history(
    session_id: i64,
    db: State<'_, Arc<Mutex<Connection>>>,
) -> Result<Vec<ChatMessage>> {
    let conn = db.lock().await;
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, message, timestamp FROM chat_history WHERE session_id = ?1 ORDER BY id ASC",
    )?;

    let messages_iter = stmt.query_map(params![session_id], |row| {
        Ok(ChatMessage {
            id: row.get(0)?,
            session_id: row.get(1)?,
            role: row.get(2)?,
            message: row.get(3)?,
            timestamp: row.get(4)?,
        })
    })?;

    let mut messages = Vec::new();
    for message in messages_iter {
        messages.push(message?);
    }

    Ok(messages)
}

// Save chat history
pub async fn save_chat_message(
    session_id: i64,
    role: &str,
    message: &str,
    db: State<'_, Arc<Mutex<Connection>>>,
) -> Result<()> {

    // Ensure there's an active session
    if session_id <= 0 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No active chat session found.",
        ))));
    }
    
    let conn = db.lock().await;

    conn.execute(
        "INSERT INTO chat_history (session_id, role, message) VALUES (?1, ?2, ?3)",
        params![session_id, role, message],
    )
    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Failed to save chat history: {}", e),
    ))))?;

    Ok(())
}
