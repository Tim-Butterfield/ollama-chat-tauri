// Handles communication with external AI API (Ollama)

use crate::db;
use crate::commands::load_chat_history;
use crate::session::GenerationState;

use tauri::State;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use reqwest::Client;
use serde::Deserialize;
use regex::Regex;
use futures_util::StreamExt;

#[derive(Deserialize)]
pub struct AIResponse {
    pub response: String,
    pub done: bool,
}

pub async fn fetch_models() -> Result<Vec<String>, String> {
    let client = Client::new();
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


// Generate a chat session title
pub async fn generate_session_title_with_ai(prompt: &str, model: &str) -> Result<String, String> {
    let client = Client::new();

    let request_body = serde_json::json!({
        "model": model,
        "prompt": format!(
            "Generate a concise and informative title (at most 10 words) summarizing the prompt. 
            Respond with only the title as plain text. Do not include any explanations, formatting, 
            or additional content. The prompt to summarize is: ```{}```",
            prompt
        )
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

    let final_title = format!("{}: {}", model, title);

    Ok(final_title)
}

pub async fn process_chat_generation(
    prompt: String,
    model: String,
    state: State<'_, Arc<Mutex<GenerationState>>>,
    db_conn: State<'_, Arc<Mutex<rusqlite::Connection>>>,
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

            let new_session_id = db::get_or_create_session(db_conn.inner(), &generated_title)
                .await
                .map_err(|e| format!("Failed to create or retrieve session: {}", e))?;

            generation_state.current_session_id = Some(new_session_id);
        }
    }

    let session_id = {
        let state_guard = state.lock().await;
        state_guard.current_session_id.unwrap_or(-1)
    };

    // save user prompt in chat history
    db::save_chat_message(session_id, "user", &prompt, db_conn.clone())
        .await
        .map_err(|e| format!("Failed to save user message: {}", e))?;

    let messages = load_chat_history(state.clone(), db_conn.clone())
        .await
        .unwrap_or_else(|_| Vec::new());

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

    // Save assistant response in chat history
    db::save_chat_message(session_id, "assistant", &ai_response, db_conn.clone())
        .await
        .map_err(|e| format!("Failed to save assistant message: {}", e))?;

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


