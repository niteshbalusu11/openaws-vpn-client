mod app;
mod cmd;
mod config;
mod consts;
mod dns;
mod ffi;
mod local_config;
mod log;
mod manager;
mod saml_server;
mod state_manager;
mod storage;
mod task;

use crate::app::VpnApp;
use crate::ffi::*;

fn main() {
    println!("This is a library meant to be used from FFI, not a standalone app!");
    println!("Please see the examples directory for usage information.");
}
