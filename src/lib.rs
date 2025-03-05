// Export all modules
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
mod task;

// Re-export ffi module for external use
pub use ffi::*;

#[cfg(target_os = "android")]
extern crate libc;

// Add Android-specific utils
#[cfg(target_os = "android")]
mod android {
    use std::ffi::CString;

    // Log levels match android/log.h values
    pub const ANDROID_LOG_VERBOSE: i32 = 2;
    pub const ANDROID_LOG_DEBUG: i32 = 3;
    pub const ANDROID_LOG_INFO: i32 = 4;
    pub const ANDROID_LOG_WARN: i32 = 5;
    pub const ANDROID_LOG_ERROR: i32 = 6;

    pub fn log(level: i32, tag: &str, msg: &str) {
        let tag_cstr = CString::new(tag).unwrap();
        let msg_cstr = CString::new(msg).unwrap();

        unsafe {
            libc::__android_log_write(level, tag_cstr.as_ptr(), msg_cstr.as_ptr());
        }
    }

    pub fn log_info(msg: &str) {
        log(ANDROID_LOG_INFO, "OpenAwsVPN", msg);
    }

    pub fn log_error(msg: &str) {
        log(ANDROID_LOG_ERROR, "OpenAwsVPN", msg);
    }
}
