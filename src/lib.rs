//! Core library for the Mysh command-line shell.
pub mod builtins;
pub mod lexer;
pub mod shell;
pub use shell::Shell;
