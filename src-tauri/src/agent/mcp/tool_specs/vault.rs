//! Tools that change the vault itself.

use serde_json::{json, Value};

use super::tool;

pub(super) fn create_connection() -> Value {
    tool(
        "create_connection",
        "Add a connection",
        "Add a server to the user's Serverus (a sidebar bookmark), including the password, private key or passphrase when the user gave them to you and asked you to store them — they go into Serverus's encrypted vault. Only when the user allowed agents to add connections (list_servers → can_add_connections); with \"with_confirmation\" the user approves it in Serverus. The folder path is created if missing. The new connection gets its folder's AI access level (you cannot choose it; where that would hide it from you, it starts at \"ask\"). One that would get \"full\" access, or that routes through a jump host where you have \"ask\", always needs the user's approval in Serverus. Returns the new id: pass it as `server` to the other tools to connect — the user confirms the host key on the first connection. For S3, username is the access key id and password the secret key; host is the endpoint (e.g. fra1.digitaloceanspaces.com).",
        json!({
            "host": { "type": "string", "description": "Host name or IP address." },
            "name": { "type": "string", "description": "Display name; defaults to the host." },
            "folder": { "type": "string", "description": "Folder path in the sidebar, e.g. \"Clients/Acme\". Omit for the top level." },
            "protocol": { "type": "string", "enum": ["ssh", "ftp", "s3"], "default": "ssh" },
            "port": { "type": "integer", "minimum": 1, "maximum": 65535, "description": "Defaults to 22 / 21 / 443 by protocol." },
            "username": { "type": "string" },
            "auth": { "type": "string", "enum": ["password", "key", "agent"], "description": "SSH authentication. Default: key when a key is given, password when a password is given, otherwise the local ssh-agent." },
            "password": { "type": "string", "description": "Password (S3: secret access key). Stored encrypted." },
            "private_key": { "type": "string", "description": "Private key text (PEM / OpenSSH). Stored encrypted." },
            "key_path": { "type": "string", "description": "Path of a private key file on this computer." },
            "key_passphrase": { "type": "string", "description": "Passphrase of the private key." },
            "jump_host": { "type": "string", "description": "An existing SSH server to connect through (name, path or id); you need \"ask\" or \"full\" access to it." },
            "remote_dir": { "type": "string", "description": "Initial remote directory." },
            "local_dir": { "type": "string", "description": "Initial local directory." },
            "disable_terminal": { "type": "boolean", "default": false, "description": "SSH account without a shell (SFTP only)." },
            "ftp_tls": { "type": "boolean", "default": false, "description": "FTP over explicit TLS (FTPS)." },
            "s3_region": { "type": "string" },
            "s3_bucket": { "type": "string", "description": "Lock the connection to one bucket." },
            "s3_path_style": { "type": "boolean", "default": false, "description": "Path-style addressing (MinIO and similar)." },
            "notes": { "type": "string" },
            "allow_duplicate": { "type": "boolean", "default": false, "description": "Add even if a connection with the same protocol, host, port and user exists." },
        }),
        &["host"],
        false,
    )
}
