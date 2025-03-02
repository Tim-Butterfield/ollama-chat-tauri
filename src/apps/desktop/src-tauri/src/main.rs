mod db;
mod commands;
mod session;
mod ollama_api;

use rusqlite::Connection;
use tauri::{Manager, PhysicalPosition, PhysicalSize, WindowEvent};
use tokio::sync::Mutex;
use std::sync::Arc;


// Save window state
fn save_window_state(window: &tauri::Window, conn: &Connection) -> rusqlite::Result<()> {
    if let Ok(position) = window.outer_position() {
        db::update_config_value(conn, "window_x", &position.x.to_string())?;
        db::update_config_value(conn, "window_y", &position.y.to_string())?;
    }

    if let Ok(size) = window.outer_size() {
        db::update_config_value(conn, "window_width", &size.width.to_string())?;
        db::update_config_value(conn, "window_height", &size.height.to_string())?;
    }

    Ok(())
}

// Load window state
fn load_window_state(window: &tauri::Window, conn: &Connection) -> rusqlite::Result<()> {
    let x = db::get_config_value(conn, "window_x")?.unwrap_or_else(|| "100".to_string()).parse().unwrap_or(100);
    let y = db::get_config_value(conn, "window_y")?.unwrap_or_else(|| "100".to_string()).parse().unwrap_or(100);
    let width = db::get_config_value(conn, "window_width")?.unwrap_or_else(|| "1600".to_string()).parse().unwrap_or(800);
    let height = db::get_config_value(conn, "window_height")?.unwrap_or_else(|| "1440".to_string()).parse().unwrap_or(600);

    window.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)))
        .expect("Failed to set window position");

    window.set_size(PhysicalSize::new(width, height))
        .expect("Failed to set window size");

    Ok(())
}

// application entry point
fn main() {
    let db_conn = db::init_db();
    let generation_state = Arc::new(Mutex::new(session::GenerationState::default()));

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
            commands::load_models,
            commands::get_selected_model,
            commands::save_selected_model,
            commands::clear_current_session,
            commands::get_current_session,
            commands::load_chat_history,
            commands::generate_chat,
            commands::abort_generation,
            commands::delete_chat_session,
            commands::update_chat_session_name,
            commands::load_chat_sessions,
            commands::set_current_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}