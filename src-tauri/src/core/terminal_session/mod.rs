//! Terminal session implementations that bridge transports into the shared session model.

pub(crate) mod local;
pub(crate) mod serial;
pub(crate) mod telnet;

pub(crate) use nyaterm_core::terminal_encoding::{
    TerminalOutputDecoder, encode_terminal_input, prepare_terminal_write_input,
};
