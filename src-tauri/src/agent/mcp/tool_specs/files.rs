//! File and transfer tools. They work on every protocol (SFTP, FTP, S3).

use serde_json::{json, Value};

use super::{if_exists_property, server_property, tool};

pub(super) fn all() -> Vec<Value> {
    vec![
        list_directory(),
        read_file(),
        write_file(),
        upload(),
        download(),
        transfer_status(),
        make_directory(),
        rename(),
        delete(),
        chmod(),
    ]
}

fn wait_property(default: u32) -> Value {
    json!({
        "type": "integer", "minimum": 0, "maximum": 3600, "default": default,
        "description": "How long to wait for the queued items to finish. They keep going after that; check with transfer_status.",
    })
}

fn list_directory() -> Value {
    tool(
        "list_directory",
        "List a remote directory",
        "List a remote directory through Serverus (SFTP, FTP or S3 — also on servers without a shell). Entries carry type, size, modification time and permissions. Defaults to the login directory.",
        json!({
            "server": server_property(),
            "path": { "type": "string", "description": "Absolute remote path. Omit for the login directory." },
        }),
        &["server"],
        true,
    )
}

fn read_file() -> Value {
    tool(
        "read_file",
        "Read a remote file",
        "Read a remote text file. Binary files are refused — use download for those.",
        json!({
            "server": server_property(),
            "path": { "type": "string", "description": "Absolute remote path." },
            "offset": { "type": "integer", "minimum": 0, "default": 0, "description": "Byte offset to start at." },
            "max_bytes": { "type": "integer", "minimum": 1, "maximum": 4_194_304, "default": 262_144, "description": "Most bytes to return." },
        }),
        &["server", "path"],
        true,
    )
}

fn write_file() -> Value {
    tool(
        "write_file",
        "Write a remote file",
        "Create or replace a remote file with the given text. It goes through the tab's transfer queue in Serverus, so the user sees it like any upload.",
        json!({
            "server": server_property(),
            "path": { "type": "string", "description": "Absolute remote path of the file." },
            "content": { "type": "string", "description": "The complete new file content." },
            "if_exists": if_exists_property(),
        }),
        &["server", "path", "content"],
        false,
    )
}

fn upload() -> Value {
    tool(
        "upload",
        "Upload local files",
        "Upload files or whole folders (recursively) from this computer into a remote directory, through the tab's transfer queue in Serverus (the user sees progress there; large folders over SSH are tar-accelerated).",
        json!({
            "server": server_property(),
            "local_paths": { "type": "array", "items": { "type": "string" }, "minItems": 1, "description": "Absolute local paths (~ is expanded)." },
            "remote_dir": { "type": "string", "description": "Absolute remote directory to upload into." },
            "if_exists": if_exists_property(),
            "wait_seconds": wait_property(600),
        }),
        &["server", "local_paths", "remote_dir"],
        false,
    )
}

fn download() -> Value {
    tool(
        "download",
        "Download remote files",
        "Download remote files or whole folders (recursively) into a local directory on this computer, through the tab's transfer queue in Serverus. The local directory is created if missing.",
        json!({
            "server": server_property(),
            "remote_paths": { "type": "array", "items": { "type": "string" }, "minItems": 1, "description": "Absolute remote paths." },
            "local_dir": { "type": "string", "description": "Absolute local directory (~ is expanded)." },
            "if_exists": if_exists_property(),
            "wait_seconds": wait_property(600),
        }),
        &["server", "remote_paths", "local_dir"],
        false,
    )
}

fn transfer_status() -> Value {
    tool(
        "transfer_status",
        "Transfer status",
        "Show items of a server tab's transfer queue — uploads, downloads, deletes, chmods — with state and progress. Without ids, every item of the tab.",
        json!({
            "server": server_property(),
            "ids": { "type": "array", "items": { "type": "string" }, "description": "Item ids returned by upload, download, write_file, delete or chmod." },
        }),
        &["server"],
        true,
    )
}

fn make_directory() -> Value {
    tool(
        "make_directory",
        "Create a remote directory",
        "Create a remote directory (on S3: a folder marker).",
        json!({
            "server": server_property(),
            "path": { "type": "string", "description": "Absolute remote path." },
            "parents": { "type": "boolean", "default": false, "description": "Also create missing parent directories; an existing directory is fine." },
        }),
        &["server", "path"],
        false,
    )
}

fn rename() -> Value {
    tool(
        "rename",
        "Rename or move",
        "Rename or move a remote file or directory on the same server.",
        json!({
            "server": server_property(),
            "from": { "type": "string", "description": "Absolute remote path." },
            "to": { "type": "string", "description": "Absolute new remote path." },
        }),
        &["server", "from", "to"],
        false,
    )
}

fn delete() -> Value {
    tool(
        "delete",
        "Delete remote files",
        "Delete remote files or directories — directories with everything inside. Symlinks are removed as links, never followed. Runs in the tab's transfer queue, so the user sees progress.",
        json!({
            "server": server_property(),
            "paths": { "type": "array", "items": { "type": "string" }, "minItems": 1, "description": "Absolute remote paths." },
            "wait_seconds": wait_property(300),
        }),
        &["server", "paths"],
        false,
    )
}

fn chmod() -> Value {
    tool(
        "chmod",
        "Change permissions",
        "Change the permission bits of a remote file or directory (SFTP/FTP).",
        json!({
            "server": server_property(),
            "path": { "type": "string", "description": "Absolute remote path." },
            "mode": { "type": "string", "pattern": "^[0-7]{3,4}$", "description": "Octal mode, e.g. \"755\"." },
            "recursive": { "type": "boolean", "default": false, "description": "Apply to everything inside a directory too (runs in the transfer queue)." },
            "apply_to": { "type": "string", "enum": ["files", "dirs", "both"], "default": "both", "description": "With recursive: which entries to change." },
            "wait_seconds": wait_property(300),
        }),
        &["server", "path", "mode"],
        false,
    )
}
