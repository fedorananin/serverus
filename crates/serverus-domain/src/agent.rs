//! Pure rules for letting an AI agent (over MCP) work with the user's
//! servers: who may do what, how a command is typed into a shared shell so
//! its completion can be detected, and how raw terminal bytes become text an
//! agent can read.

pub mod access;
pub mod shell;
pub mod terminal_modes;
pub mod terminal_text;
