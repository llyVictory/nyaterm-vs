//! Transport-independent Telnet negotiation, editing and automatic login.
pub const IAC: u8 = 255;
pub const WILL: u8 = 251;
pub const WONT: u8 = 252;
pub const DO: u8 = 253;
pub const DONT: u8 = 254;
pub const SB: u8 = 250;
pub const SE: u8 = 240;
pub const OPT_ECHO: u8 = 1;
pub const OPT_SUPPRESS_GO_AHEAD: u8 = 3;
pub const OPT_NAWS: u8 = 31;
include!("types.rs");
include!("negotiation.rs");
include!("line_editor.rs");
include!("auto_login.rs");

include!("tests.rs");
