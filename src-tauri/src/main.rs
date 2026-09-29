// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `serverus --mcp` is the stdio MCP server an AI agent launches; it
    // relays to the running app instead of opening a window.
    if std::env::args().nth(1).as_deref() == Some(serverus_lib::agent::shim::FLAG) {
        std::process::exit(serverus_lib::agent::shim::run());
    }
    serverus_lib::run()
}
