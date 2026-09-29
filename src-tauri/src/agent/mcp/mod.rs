//! The Model Context Protocol server: protocol shapes, the per-connection
//! loop, and the advertised tool catalog.

pub mod protocol;
pub mod server;
pub mod tool_specs;

/// Server-level guidance sent in the `initialize` result.
pub const INSTRUCTIONS: &str = "Serverus is the user's SSH/SFTP/FTP/S3 connection manager. \
Its tools work on the servers the user shared with you, identified by name, Folder/Name path, id, or \"current\" (the tab the user is looking at). \
Everything happens in the user's visible Serverus window: run_command types into the server's real terminal tab (state persists between calls), \
and uploads, downloads, writes and deletes show up in that tab's transfer queue. \
If the user asks, create_connection adds a server to Serverus (secrets included) when they allowed that. \
The user can take a terminal over at any time; when a call reports that, ask them to hand it back, then call read_terminal to see what they did. \
On servers with the \"ask\" level every change waits for the user's approval in Serverus, so prefer fewer, larger steps. \
Never try to read credentials: Serverus keeps them in its encrypted vault and no tool exposes them.";
