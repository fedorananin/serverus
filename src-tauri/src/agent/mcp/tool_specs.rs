//! The tool catalog advertised to MCP clients: names, descriptions and
//! input schemas. Descriptions are written for the model that calls them.

use serde_json::{json, Map, Value};

mod files;
mod vault;

fn tool(
    name: &str,
    title: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
) -> Value {
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), properties);
    schema.insert("required".into(), json!(required));
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": schema,
        "annotations": {
            "title": title,
            "readOnlyHint": read_only,
            "destructiveHint": !read_only,
            "openWorldHint": true,
        },
    })
}

pub(super) fn server_property() -> Value {
    json!({
        "type": "string",
        "description": "Which server: \"current\" for the tab the user is looking at, or a server's name, Folder/Name path or id as listed by list_servers.",
    })
}

pub(super) fn if_exists_property() -> Value {
    json!({
        "type": "string",
        "enum": ["overwrite", "skip", "rename"],
        "default": "overwrite",
        "description": "What to do when the target already exists.",
    })
}

/// Every tool, in the order clients show them.
pub fn all() -> Value {
    let mut tools = vec![
        list_servers(),
        vault::create_connection(),
        run_command(),
        read_terminal(),
        send_input(),
    ];
    tools.extend(files::all());
    Value::Array(tools)
}

fn list_servers() -> Value {
    tool(
        "list_servers",
        "List servers",
        "List the servers the user made available to you in Serverus: name, folder path, protocol (ssh / ftp / s3), host, user, your access level and whether a tab is open (and which one is current). Access levels: read_only = listing and reading only; ask = everything, but each change needs the user's approval in Serverus; full = everything without asking. Servers the user did not share are not listed.",
        json!({
            "query": { "type": "string", "description": "Only servers whose name, folder path or host contains this text (case-insensitive)." },
        }),
        &[],
        true,
    )
}

fn run_command() -> Value {
    tool(
        "run_command",
        "Run a shell command",
        "Run a shell command on an SSH server in the user's own terminal tab in Serverus — the tab for that server is reused if open, otherwise opened. The user watches it live and shell state persists between calls (working directory, environment, sudo, activated virtualenvs), so you can continue where the user left off and vice versa. Returns the exit status and the output (stdout and stderr interleaved, colors stripped, very long output cut in the middle). If the command runs longer than timeout_seconds it keeps running: follow it with read_terminal(wait_seconds) or stop it with send_input(keys=[\"ctrl-c\"]). Prompts (sudo password, y/n) can be answered by the user in Serverus or by you with send_input. Prefer non-interactive flags (-y, --no-pager). Refused while the user has taken control of the terminal or when it is not at a shell prompt (force=true types anyway).",
        json!({
            "server": server_property(),
            "command": { "type": "string", "description": "The command line. Multi-line scripts and heredocs are fine." },
            "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 3600, "default": 60, "description": "How long to wait for completion before returning with the command still running." },
            "force": { "type": "boolean", "default": false, "description": "Type the command even if the terminal does not look idle, replacing a previous command of yours that is still being followed." },
            "shell": { "type": "string", "enum": ["posix", "fish", "csh"], "description": "Syntax of the shell running in this terminal. Serverus detects the login shell itself; pass this only when a call reported that the shell rejected the command (the user started another shell in the tab). Remembered for the terminal." },
        }),
        &["server", "command"],
        false,
    )
}

fn read_terminal() -> Value {
    tool(
        "read_terminal",
        "Read the terminal",
        "Show what a server's terminal in Serverus contains: its recent output as plain text plus its state (idle at a prompt, busy, running your command, a full-screen program, or taken over by the user). Use it to see what the user did while they had control, to check on a long command, or to wait for it (wait_seconds).",
        json!({
            "server": server_property(),
            "lines": { "type": "integer", "minimum": 1, "maximum": 2000, "default": 100, "description": "How many trailing lines to return." },
            "wait_seconds": { "type": "integer", "minimum": 0, "maximum": 3600, "default": 0, "description": "Wait up to this long for your running command to finish first." },
        }),
        &["server"],
        true,
    )
}

fn send_input() -> Value {
    tool(
        "send_input",
        "Send keystrokes",
        "Type into a server's terminal without waiting for anything to finish: answer a prompt, drive an interactive program, or interrupt with ctrl-c. `text` is typed verbatim (it is not submitted — add keys=[\"enter\"]); `keys` are sent after it, in order. Returns the terminal's last lines shortly after.",
        json!({
            "server": server_property(),
            "text": { "type": "string", "description": "Text to type as-is." },
            "keys": {
                "type": "array",
                "items": { "type": "string", "enum": [
                    "enter", "tab", "escape", "backspace", "delete", "space",
                    "up", "down", "left", "right", "home", "end", "page_up", "page_down",
                    "ctrl-a", "ctrl-b", "ctrl-c", "ctrl-d", "ctrl-e", "ctrl-f", "ctrl-g", "ctrl-h",
                    "ctrl-k", "ctrl-l", "ctrl-n", "ctrl-o", "ctrl-p", "ctrl-r", "ctrl-t", "ctrl-u",
                    "ctrl-w", "ctrl-x", "ctrl-y", "ctrl-z", "ctrl-\\"
                ] },
                "description": "Special keys to press after the text.",
            },
        }),
        &["server"],
        false,
    )
}
