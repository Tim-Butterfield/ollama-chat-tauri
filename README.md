
# OllamaChat

A fully offline, cross-platform chat application using **React.js**, **Tauri**, **Rust**, and **SQLite** for the desktop app.  
The app integrates with the Ollama API to retrieve models and generate responses.

Caveat:

The current feature set is rudimentary and only lightly tested.  Expect missing features and defects.

## Ollama

Download Ollama here: https://ollama.com/

Load an Ollama model like this:

```bash
ollama run llama3.2:1b
```

Make sure Ollama API is running

```bash
ollama serve
```

The rust code is currently hard-coded for port 11434.

## Installation

```bash
git clone https://github.com/Tim-Butterfield/ollama-chat-tauri.git
cd ollama-chat-tauri
cd src/apps/desktop
npm install
npm run tauri
```
