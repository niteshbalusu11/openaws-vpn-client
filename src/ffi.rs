use crate::app::{State, VpnApp};
use crate::cmd::{kill_openvpn, ProcessInfo};
use crate::local_config::LocalConfig;
use crate::manager::ConnectionManager;
use crate::saml_server::SamlServer;
use std::ffi::{c_char, CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Global state for FFI
static mut APP_INSTANCE: Option<Arc<Mutex<Rc<VpnApp>>>> = None;

#[no_mangle]
pub extern "C" fn openaws_init() -> bool {
    initialize_app()
}

fn initialize_app() -> bool {
    let vpn_app = Rc::new(VpnApp::new());

    // Setup connection manager
    let connection_manager = ConnectionManager::new();
    connection_manager.set_app(vpn_app.clone());
    vpn_app.set_connection_manager(connection_manager);

    // Start SAML server
    let saml_server = SamlServer::new();
    saml_server.start_server(vpn_app.clone());

    // Check for lingering OpenVPN sessions
    if let Some(p) = LocalConfig::read_last_pid() {
        vpn_app.log.append(format!(
            "Last OpenVPN session (PID: {}) was not closed properly",
            p
        ));
        let process_info = Arc::new(ProcessInfo::new());
        let pid = p;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            kill_openvpn(pid);
        });
    }

    unsafe {
        APP_INSTANCE = Some(Arc::new(Mutex::new(vpn_app)));
    }

    true
}

#[no_mangle]
pub unsafe extern "C" fn openaws_connect(config_path: *const c_char) -> bool {
    if config_path.is_null() {
        log_to_android("Error: config_path is null");
        return false;
    }

    let c_str = CStr::from_ptr(config_path);
    let config_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => {
            log_to_android("Error: config_path is not a valid UTF-8 string");
            return false;
        }
    };

    let app = match &APP_INSTANCE {
        Some(app) => app.lock().unwrap().clone(),
        None => {
            log_to_android("Error: call openaws_init() first");
            return false;
        }
    };

    let config_path = PathBuf::from(config_str);
    if !config_path.exists() {
        app.log
            .append(format!("Config file does not exist: {}", config_str));
        log_to_android(&format!("Config file does not exist: {}", config_str));
        return false;
    }

    app.log
        .append(format!("Connecting using config: {}", config_str));
    app.config.save_config(&config_path);
    app.dns.resolve_addresses();

    // Initiate connection
    let manager = app.connection_manager.lock().unwrap();
    if let Some(ref manager) = *manager {
        manager.connect();
        return true;
    }

    false
}

#[no_mangle]
pub extern "C" fn openaws_disconnect() -> bool {
    let app = unsafe {
        match &APP_INSTANCE {
            Some(app) => app.lock().unwrap().clone(),
            None => {
                log_to_android("Error: call openaws_init() first");
                return false;
            }
        }
    };

    let manager = app.connection_manager.lock().unwrap();
    if let Some(ref manager) = *manager {
        manager.disconnect();
        return true;
    }

    false
}

#[no_mangle]
pub extern "C" fn openaws_get_state() -> i32 {
    let app = unsafe {
        match &APP_INSTANCE {
            Some(app) => app.lock().unwrap().clone(),
            None => {
                log_to_android("Error: call openaws_init() first");
                return -1;
            }
        }
    };

    let state = {
        let state_manager = app.state.lock().unwrap();
        if let Some(ref state_manager) = *state_manager {
            *state_manager.state.borrow()
        } else {
            return -1;
        }
    };

    match state {
        State::Disconnected => 0,
        State::Connecting => 1,
        State::Connected => 2,
    }
}

#[no_mangle]
pub unsafe extern "C" fn openaws_get_last_log() -> *mut c_char {
    let app = match &APP_INSTANCE {
        Some(app) => app.lock().unwrap().clone(),
        None => return ptr::null_mut(),
    };

    // Create a temporary container to store the log message
    let log_message = Arc::new(Mutex::new(String::new()));
    let cloned_log = log_message.clone();

    // Extract the log message using a callback
    app.log.get_last_log(Box::new(move |msg: &str| {
        let mut log = cloned_log.lock().unwrap();
        *log = msg.to_string();
    }));

    // Return the log message
    let buffer = log_message.lock().unwrap().clone();
    if buffer.is_empty() {
        return ptr::null_mut();
    }

    match CString::new(buffer) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn openaws_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[no_mangle]
pub extern "C" fn openaws_cleanup() -> bool {
    let app = unsafe {
        match &APP_INSTANCE {
            Some(app) => app.lock().unwrap().clone(),
            None => {
                log_to_android("Error: App not initialized");
                return false;
            }
        }
    };

    // Clean up when exiting
    let manager = app.connection_manager.lock().unwrap();
    if let Some(manager) = manager.as_ref() {
        manager.force_disconnect();
    }

    unsafe {
        APP_INSTANCE = None;
    }

    true
}

// Log to Android logcat if on Android
#[cfg(target_os = "android")]
fn log_to_android(msg: &str) {
    use std::ffi::CString;

    unsafe {
        let tag = CString::new("OpenAwsVPN").unwrap();
        let message = CString::new(msg).unwrap();
        // ANDROID_LOG_INFO = 4
        libc::__android_log_write(4, tag.as_ptr(), message.as_ptr());
    }
}

#[cfg(not(target_os = "android"))]
fn log_to_android(msg: &str) {
    println!("Android: {}", msg);
}
