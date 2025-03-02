use crate::app::State;
use crate::cmd::{kill_openvpn, ProcessInfo};
use crate::config::Config;
use crate::saml_server::Saml;
use crate::task::OavcProcessTask;
use crate::LocalConfig;
use libc::{c_char, c_int, c_uint, size_t};
use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

/// Status of the VPN connection
#[repr(C)]
pub enum VpnStatus {
    Disconnected = 0,
    Connecting = 1,
    Connected = 2,
    Error = 3,
}

impl From<State> for VpnStatus {
    fn from(state: State) -> Self {
        match state {
            State::Disconnected => VpnStatus::Disconnected,
            State::Connecting => VpnStatus::Connecting,
            State::Connected => VpnStatus::Connected,
        }
    }
}

/// Credentials for SAML authentication
#[repr(C)]
pub struct VpnCredentials {
    pub username: *const c_char,
    pub password: *const c_char,
}

/// Configuration for VPN connection
#[repr(C)]
pub struct VpnConfig {
    pub config_path: *const c_char,
    pub server_address: *const c_char,
    pub port: c_uint,
}

/// Opaque handle to the VPN client
#[repr(C)]
pub struct VpnClientHandle {
    pub runtime: Arc<Runtime>,
    pub config: Arc<Config>,
    pub process_info: Arc<ProcessInfo>,
    pub connection: Arc<Mutex<Option<OavcProcessTask<i32>>>>,
    pub status: Arc<Mutex<VpnStatus>>,
    pub callback: Option<extern "C" fn(status: VpnStatus, user_data: *mut libc::c_void)>,
    pub callback_data: *mut libc::c_void,
}

unsafe impl Send for VpnClientHandle {}
unsafe impl Sync for VpnClientHandle {}

/// Creates a new VPN client instance
#[no_mangle]
pub extern "C" fn openaws_vpn_client_new() -> *mut VpnClientHandle {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    );

    let config = Arc::new(Config::new());
    let process_info = Arc::new(ProcessInfo::new());

    let handle = Box::new(VpnClientHandle {
        runtime,
        config,
        process_info,
        connection: Arc::new(Mutex::new(None)),
        status: Arc::new(Mutex::new(VpnStatus::Disconnected)),
        callback: None,
        callback_data: ptr::null_mut(),
    });

    Box::into_raw(handle)
}

/// Sets a status change callback
#[no_mangle]
pub extern "C" fn openaws_vpn_client_set_status_callback(
    client: *mut VpnClientHandle,
    callback: Option<extern "C" fn(status: VpnStatus, user_data: *mut libc::c_void)>,
    user_data: *mut libc::c_void,
) {
    let client = unsafe {
        assert!(!client.is_null());
        &mut *client
    };

    client.callback = callback;
    client.callback_data = user_data;
}

/// Sets the VPN configuration
#[no_mangle]
pub extern "C" fn openaws_vpn_client_set_config(
    client: *mut VpnClientHandle,
    config: VpnConfig,
) -> c_int {
    let client = unsafe {
        if client.is_null() {
            return -1;
        }
        &mut *client
    };

    let config_path = unsafe {
        if config.config_path.is_null() {
            return -1;
        }
        CStr::from_ptr(config.config_path)
            .to_string_lossy()
            .into_owned()
    };

    let server_address = unsafe {
        if config.server_address.is_null() {
            return -1;
        }
        CStr::from_ptr(config.server_address)
            .to_string_lossy()
            .into_owned()
    };

    // Save the configuration
    client.config.save_config(PathBuf::from(&config_path));

    // Update the remote server
    let mut remote = client.config.remote.lock().unwrap();
    *remote = Some((server_address, config.port as u16));

    0
}

/// Connects to the VPN using SAML authentication
#[no_mangle]
pub extern "C" fn openaws_vpn_client_connect_saml(
    client: *mut VpnClientHandle,
    saml_response: *const c_char,
    saml_password: *const c_char,
) -> c_int {
    let client = unsafe {
        if client.is_null() {
            return -1;
        }
        &mut *client
    };

    let saml_response = unsafe {
        if saml_response.is_null() {
            return -1;
        }
        CStr::from_ptr(saml_response).to_string_lossy().into_owned()
    };

    let saml_password = unsafe {
        if saml_password.is_null() {
            return -1;
        }
        CStr::from_ptr(saml_password).to_string_lossy().into_owned()
    };

    // Update status
    {
        let mut status = client.status.lock().unwrap();
        *status = VpnStatus::Connecting;

        // Call the callback if set
        if let Some(callback) = client.callback {
            callback(VpnStatus::Connecting, client.callback_data);
        }
    }

    // Get the config and remote settings
    let config_path = {
        let config = client.config.config.lock().unwrap();
        match &*config {
            Some(path) => path.clone(),
            None => return -1,
        }
    };

    let (server, port) = {
        let remote = client.config.remote.lock().unwrap();
        match &*remote {
            Some((addr, port)) => (addr.clone(), *port),
            None => return -1,
        }
    };

    let saml = Saml {
        data: saml_response,
        pwd: saml_password,
    };

    let process_info = client.process_info.clone();
    let connection_clone = client.connection.clone();
    let status_clone = client.status.clone();
    let callback = client.callback;
    let callback_data = client.callback_data;

    // Start connection in the runtime
    let handle = client.runtime.spawn(async move {
        let result = crate::cmd::connect_ovpn(
            Arc::new(crate::Log::new()), // Dummy log for library
            config_path,
            server,
            port,
            saml,
            process_info,
        )
        .await;

        // Update status based on result
        let new_status = if result == 0 {
            VpnStatus::Connected
        } else {
            VpnStatus::Error
        };

        let mut status = status_clone.lock().unwrap();
        *status = new_status;

        // Call the callback if set
        if let Some(cb) = callback {
            cb(new_status, callback_data);
        }

        result
    });

    let task = OavcProcessTask::new(
        "OpenVPN Connection".to_string(),
        handle,
        Arc::new(crate::Log::new()), // Dummy log for library
        client.process_info.clone(),
    );

    let mut connection = connection_clone.lock().unwrap();
    *connection = Some(task);

    0
}

/// Disconnects from the VPN
#[no_mangle]
pub extern "C" fn openaws_vpn_client_disconnect(client: *mut VpnClientHandle) -> c_int {
    let client = unsafe {
        if client.is_null() {
            return -1;
        }
        &mut *client
    };

    let mut connection = client.connection.lock().unwrap();
    if let Some(ref conn) = *connection {
        conn.abort(true);
        *connection = None;

        // Update status
        let mut status = client.status.lock().unwrap();
        *status = VpnStatus::Disconnected;

        // Call the callback if set
        if let Some(callback) = client.callback {
            callback(VpnStatus::Disconnected, client.callback_data);
        }

        0
    } else {
        // Already disconnected
        0
    }
}

/// Gets the current status of the VPN connection
#[no_mangle]
pub extern "C" fn openaws_vpn_client_get_status(client: *const VpnClientHandle) -> VpnStatus {
    let client = unsafe {
        if client.is_null() {
            return VpnStatus::Error;
        }
        &*client
    };

    let status = client.status.lock().unwrap();
    *status
}

/// Frees resources used by the VPN client
#[no_mangle]
pub extern "C" fn openaws_vpn_client_free(client: *mut VpnClientHandle) {
    if !client.is_null() {
        let client = unsafe { Box::from_raw(client) };

        // Make sure to disconnect first
        let mut connection = client.connection.lock().unwrap();
        if let Some(ref conn) = *connection {
            conn.abort(true);
        }

        // Resources will be dropped when the Box is dropped
    }
}

/// Get the URL for SAML authentication
#[no_mangle]
pub extern "C" fn openaws_vpn_client_get_saml_url(
    client: *mut VpnClientHandle,
    out_url: *mut *mut c_char,
    out_password: *mut *mut c_char,
) -> c_int {
    let client = unsafe {
        if client.is_null() || out_url.is_null() || out_password.is_null() {
            return -1;
        }
        &mut *client
    };

    // Get the config and remote settings
    let config_path = {
        let config = client.config.config.lock().unwrap();
        match &*config {
            Some(path) => path.clone(),
            None => return -1,
        }
    };

    let (server, port) = {
        let remote = client.config.remote.lock().unwrap();
        match &*remote {
            Some((addr, port)) => (addr.clone(), *port),
            None => return -1,
        }
    };

    // Create a dummy log
    let log = Arc::new(crate::Log::new());

    // Use a runtime block to run the async function
    let runtime = &client.runtime;
    let auth_result =
        runtime.block_on(async { crate::cmd::run_ovpn(log, config_path, server, port).await });

    // Convert the result to C strings
    let url_cstring = match CString::new(auth_result.url) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let pwd_cstring = match CString::new(auth_result.pwd) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    // Transfer ownership to caller
    unsafe {
        *out_url = url_cstring.into_raw();
        *out_password = pwd_cstring.into_raw();
    }

    0
}

/// Free a string allocated by the library
#[no_mangle]
pub extern "C" fn openaws_vpn_client_free_string(string: *mut c_char) {
    if !string.is_null() {
        unsafe {
            let _ = CString::from_raw(string);
        }
    }
}
