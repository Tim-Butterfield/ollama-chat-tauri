use tauri::{Manager, PhysicalPosition, PhysicalSize, WindowEvent};
use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use std::sync::Arc;
use reqwest;
use reqwest::Client;
use futures_util::StreamExt;
use serde::Serialize;
use serde::Deserialize;
use regex::Regex;

#[derive(Serialize)]
struct ChatSession {
    id: i64,
    title: String,
}

#[derive(Serialize)]
struct CurrentSession {
    id: i64,
    title: String,
}

#[derive(Deserialize)]
struct AIResponse {
    response: String,
    done: bool,
}

struct GenerationState {
    is_running: bool,
    current_session_id: Option<i64>,
    cancellation_token: Option<CancellationToken>,
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

// Initialize SQLite Database
fn init_db() -> Arc<Mutex<Connection>> {
    let conn = Connection::open("ollamachat.db").expect("Failed to open SQLite database");

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
            user_message TEXT NOT NULL,
            model_response TEXT NOT NULL,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (session_id) REFERENCES chat_sessions(id)
        )",
        [],
    ).expect("Failed to create chat_history table");

    Arc::new(Mutex::new(conn))
}

/// Inserts or updates a configuration key-value pair.
fn update_config_value(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    //println!("Updating config value: key = {}, value = {}", 
    //    key, value);

    conn.execute(
        "INSERT OR REPLACE INTO app_config (key, value) VALUES (?1, ?2)",
        [key, value],
    )?;
    Ok(())
}

/// Retrieves a configuration value by key. Returns `None` if the key doesn't exist.
fn get_config_value(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM app_config WHERE key = ?1",
        [key],
        |row| row.get(0),
    ).optional()
}

// Save window state
fn save_window_state(window: &tauri::Window, conn: &Connection) -> rusqlite::Result<()> {
    if let Ok(position) = window.outer_position() {
        update_config_value(conn, "window_x", &position.x.to_string())?;
        update_config_value(conn, "window_y", &position.y.to_string())?;
    }

    if let Ok(size) = window.outer_size() {
        update_config_value(conn, "window_width", &size.width.to_string())?;
        update_config_value(conn, "window_height", &size.height.to_string())?;
    }

    Ok(())
}

// Load window state
fn load_window_state(window: &tauri::Window, conn: &Connection) -> rusqlite::Result<()> {
    let x = get_config_value(conn, "window_x")?.unwrap_or_else(|| "100".to_string()).parse().unwrap_or(100);
    let y = get_config_value(conn, "window_y")?.unwrap_or_else(|| "100".to_string()).parse().unwrap_or(100);
    let width = get_config_value(conn, "window_width")?.unwrap_or_else(|| "1600".to_string()).parse().unwrap_or(800);
    let height = get_config_value(conn, "window_height")?.unwrap_or_else(|| "1440".to_string()).parse().unwrap_or(600);

    window.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)))
        .expect("Failed to set window position");

    window.set_size(PhysicalSize::new(width, height))
        .expect("Failed to set window size");

    Ok(())
}

// Load available Ollama models
#[tauri::command]
async fn load_models() -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let res = client.get("http://localhost:11434/api/tags").send().await;

    match res {
        Ok(response) => {
            let data: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
            let models = data["models"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                .collect();
            Ok(models)
        }
        Err(_) => Err("Failed to load models".into()),
    }
}

// Save selected model
#[tauri::command]
async fn save_selected_model(
    conn: tauri::State<'_, Arc<Mutex<Connection>>>,
    model_name: String,
) -> Result<(), String> {
    let conn = conn.lock().await;
    update_config_value(&conn, "selected_model_name", &model_name)
        .map_err(|e| e.to_string())?;
    Ok(())
}

// Get selected model
#[tauri::command]
async fn get_selected_model(conn: tauri::State<'_, Arc<Mutex<Connection>>>) -> Result<String, String> {
    let conn = conn.lock().await;
    get_config_value(&conn, "selected_model_name")
        .map(|model| model.unwrap_or_else(|| "".to_string()))
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_chat_session(
    session_id: i64,
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    state: tauri::State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<(), String> {
    let conn = db.lock().await;
    conn.execute(
        "DELETE FROM chat_sessions WHERE id = ?1",
        rusqlite::params![session_id],
    )
    .map_err(|e| format!("Failed to delete session: {}", e))?;

    // Check if the deleted session is the current session
    let mut generation_state = state.lock().await;
    if generation_state.current_session_id == Some(session_id) {
        generation_state.current_session_id = Some(-1);  // Clear the current session
    }

    Ok(())
}

#[tauri::command]
async fn get_current_session(
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
    state: tauri::State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<CurrentSession, String> {
    let gen_state = state.lock().await;

    match gen_state.current_session_id {
        Some(id) if id != -1 => {
            let conn = db.lock().await;
            let mut stmt = conn
                .prepare("SELECT title FROM chat_sessions WHERE id = ?1")
                .map_err(|e| format!("Failed to prepare query: {}", e))?;

            let title: Option<String> = stmt
                .query_row([id], |row| row.get(0))
                .optional()
                .map_err(|e| format!("Failed to fetch title: {}", e))?;

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

#[tauri::command]
async fn update_chat_session_name(
    session_id: i64,
    new_name: String,
    db: tauri::State<'_, Arc<Mutex<Connection>>>,
) -> Result<(), String> {
    let conn = db.lock().await;
    conn.execute(
        "UPDATE chat_sessions SET title = ?1 WHERE id = ?2",
        rusqlite::params![new_name, session_id],
    )
    .map_err(|e| format!("Failed to update session name: {}", e))?;

    Ok(())
}

#[tauri::command]
async fn load_chat_sessions(db: tauri::State<'_, Arc<Mutex<Connection>>>) -> Result<Vec<ChatSession>, String> {
    let conn = db.lock().await;

    let mut stmt = conn.prepare("SELECT id, title FROM chat_sessions ORDER BY id DESC").map_err(|e| e.to_string())?;
    let sessions_iter = stmt.query_map([], |row| {
        Ok(ChatSession {
            id: row.get(0)?,
            title: row.get(1)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut sessions = Vec::new();
    for session in sessions_iter {
        sessions.push(session.map_err(|e| e.to_string())?);
    }

    Ok(sessions)
}

#[tauri::command]
async fn clear_current_session(
    state: tauri::State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<(), String> {
    let mut gen_state = state.lock().await;
    gen_state.current_session_id = Some(-1);
    Ok(())
}

#[tauri::command]
async fn set_current_session(
    session_id: i64,
    state: tauri::State<'_, Arc<Mutex<GenerationState>>>,
) -> Result<(), String> {
    let mut gen_state = state.lock().await;
    gen_state.current_session_id = Some(session_id);
    Ok(())
}

// Generate a chat session title
async fn generate_session_title_with_ai(prompt: &str, model: &str) -> Result<String, String> {
    let client = Client::new();

    let request_body = serde_json::json!({
        "model": model,
        "prompt": format!("Given the prompt delimited by triple backticks, generate a summary title containing no more than 10 words.  Format the response as a simple text string.  Other than the title, do not include any other output.  Prompt text: ```{}```", prompt)
    });

    let response = client
        .post("http://localhost:11434/api/generate")
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    let mut full_response = String::new();

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    if let Ok(parsed) = serde_json::from_str::<AIResponse>(line) {
                        full_response.push_str(&parsed.response);
                        if parsed.done {
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                return Err(format!("Error reading stream: {}", e));
            }
        }
    }

    let re_think = Regex::new(r"(?s)^<think>.*?</think>(\s*)").unwrap();
    let title = re_think.replace(&full_response, "").trim_matches('"').trim_matches('*').to_string();

    Ok(title)
}

async fn get_or_create_session(conn: &Arc<Mutex<Connection>>, title: &str) -> Result<i64, String> {
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

// call to generate an AI response
#[tauri::command]
async fn generate_chat(
    prompt: String,
    model: String,
    state: tauri::State<'_, Arc<Mutex<GenerationState>>>,
    db_conn: tauri::State<'_, Arc<Mutex<Connection>>>
) -> Result<String, String> {
    let cancellation_token;

    {
        let mut generation_state = state.lock().await;
        generation_state.is_running = true;
        generation_state.cancellation_token = Some(CancellationToken::new());
        cancellation_token = generation_state.cancellation_token.clone().unwrap();

        if generation_state.current_session_id.is_none() || generation_state.current_session_id == Some(-1) {
            let generated_title = generate_session_title_with_ai(&prompt, &model)
                .await
                .map_err(|e| format!("Failed to generate session title: {}", e))?;

            let new_session_id = get_or_create_session(db_conn.inner(), &generated_title)
                .await
                .map_err(|e| format!("Failed to create or retrieve session: {}", e))?;

            generation_state.current_session_id = Some(new_session_id);
        }
    }

    // Load previous chat history for context
    let mut messages = load_chat_history(state.clone(), db_conn.clone())
        .await
        .unwrap_or_else(|_| Vec::new());

    // Add the new user prompt to the messages array
    messages.push(serde_json::json!({
        "role": "user",
        "content": prompt
    }));

    let session_id = {
        let state_guard = state.lock().await;
        state_guard.current_session_id.unwrap_or(-1)
    };

    let mut ai_response = String::new();

    let generation_result: Result<(), String> = tokio::select! {
        result = async {
            let client = reqwest::Client::new();

            let response = client
                .post("http://localhost:11434/api/chat")
                .json(&serde_json::json!({
                    "model": model,
                    "messages": messages
                }))
                .send()
                .await
                .map_err(|e| format!("Failed to make API call: {}", e))?;

            if !response.status().is_success() {
                return Err(format!("API call failed with status: {}", response.status()));
            }

            let mut stream = response.bytes_stream();
            loop {
                tokio::select! {
                    chunk = stream.next() => {
                        if let Some(chunk) = chunk {
                            let data = chunk.map_err(|e| e.to_string())?;
                            let text_chunk = String::from_utf8_lossy(&data);

                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text_chunk) {
                                if let Some(text) = json.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                                    ai_response.push_str(text);
                                }

                                if json.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
                                    break;
                                }
                            }
                        } else {
                            break;
                        }
                    },
                    _ = cancellation_token.cancelled() => {
                        ai_response.push_str("\n\nCancelled\n");
                        println!("Generation task was cancelled");
                        break;
                    }
                }
            }
            Ok(())
        } => {
            result
        },
        _ = cancellation_token.cancelled() => {
            println!("Cancellation token triggered");
            Ok(())
        }
    };

    save_chat_history(prompt, ai_response.clone(), session_id, db_conn)
        .await
        .map_err(|e| format!("Failed to save chat history: {}", e))?;

    let mut generation_state = state.lock().await;
    generation_state.is_running = false;
    generation_state.cancellation_token = None;

    match generation_result {
        Ok(_) => Ok(ai_response),
        Err(e) => {
            println!("Error generating chat: {}", e);
            Err(e)
        }
    }
}

// Load chat history
#[tauri::command]
async fn load_chat_history(
    state: tauri::State<'_, Arc<Mutex<GenerationState>>>,
    db_conn: tauri::State<'_, Arc<Mutex<Connection>>>
) -> Result<Vec<serde_json::Value>, String> {
    let generation_state = state.lock().await;

    let session_id = generation_state
        .current_session_id
        .ok_or_else(|| "No active chat session found.".to_string())?;

    let mut history = Vec::new();

    if session_id > -1 {
        let conn = db_conn.lock().await;

        let mut stmt = conn
            .prepare("SELECT user_message, model_response FROM chat_history WHERE session_id = ?1 ORDER BY id ASC")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let rows = stmt
            .query_map(params![session_id], |row| {
                let user_message: String = row.get(0)?;
                let model_response: String = row.get(1)?;

                // Return two JSON objects for user and AI/assistant
                Ok(vec![
                    serde_json::json!({
                        "role": "user",
                        "content": user_message
                    }),
                    serde_json::json!({
                        "role": "assistant",
                        "content": model_response
                    }),
                ])
            })
            .map_err(|e| format!("Failed to load chat history: {}", e))?;

        for entry in rows {
            history.extend(entry.map_err(|e| format!("Failed to parse message: {}", e))?);
        }
    }

    Ok(history)
}

// Save chat history
async fn save_chat_history(
    prompt: String,
    response: String,
    session_id: i64,
    db_conn: tauri::State<'_, Arc<Mutex<Connection>>>
) -> Result<(), String> {

    // Ensure there's an active session
    if session_id <= 0 {
        return Err("No active chat session found.".to_string());
    }

    let conn = db_conn.lock().await;

    //println!("Saving chat history: session_id = {}, prompt = {}, response = {}", session_id, prompt, response);
    
    conn.execute(
        "INSERT INTO chat_history (session_id, user_message, model_response) VALUES (?1, ?2, ?3)",
        rusqlite::params![session_id, prompt, response],
    )
    .map_err(|e| format!("Failed to save chat history: {}", e))?;

    Ok(())
}

// Abort chat generation
#[tauri::command]
async fn abort_generation(state: tauri::State<'_, Arc<Mutex<GenerationState>>>) -> Result<(), String> {
    let mut generation_state = state.lock().await;

    if let Some(token) = &generation_state.cancellation_token {
        token.cancel(); // Trigger cancellation
    }

    generation_state.is_running = false; // Update state to indicate generation is no longer running
    generation_state.cancellation_token = None; // Clear the cancellation token

    Ok(())
}

// application entry point
fn main() {
    let db_conn = init_db();
    let generation_state = Arc::new(Mutex::new(GenerationState::default()));
        
    tauri::Builder::default()
        .manage(db_conn.clone())
        .manage(generation_state)
        .setup(move |app| {
            let window = app.get_window("main").unwrap();

            // Clone before moving into the async block
            let window_clone_for_async = window.clone();
            let db_conn_clone_for_async = db_conn.clone();
            
            tauri::async_runtime::block_on(async {
                load_window_state(&window_clone_for_async, &*db_conn_clone_for_async.lock().await)
                    .expect("Failed to load window state");
            });
        
            // Safe to use the original window and db_conn here
            let window_clone = window.clone();
            let db_conn_clone = db_conn.clone();
            
            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::Resized(_) | WindowEvent::Moved(_)) {
                    let window_clone_inner = window_clone.clone();
                    let db_conn_clone_inner = db_conn_clone.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = save_window_state(&window_clone_inner, &*db_conn_clone_inner.lock().await) {
                            eprintln!("Failed to save window state: {}", e);
                        }
                    });
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_models,
            get_selected_model,
            save_selected_model,
            clear_current_session,
            get_current_session,
            load_chat_history,
            generate_chat,
            abort_generation,
            delete_chat_session,
            update_chat_session_name,
            load_chat_sessions,
            set_current_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}