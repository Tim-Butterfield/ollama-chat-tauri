use std::env;

fn main() {
    let db_path = env::current_dir().unwrap().join("../ollamachat.db");
    let sql_path = env::current_dir().unwrap().join("../init.sql");

    // Remove debug logs, just check files
    if !db_path.exists() {
        panic!("ollamachat.db NOT FOUND ❌");
    }

    if !sql_path.exists() {
        panic!("init.sql NOT FOUND ❌");
    }

    println!("cargo:rerun-if-changed=../ollamachat.db");
    println!("cargo:rerun-if-changed=../init.sql");

    tauri_build::build();
}