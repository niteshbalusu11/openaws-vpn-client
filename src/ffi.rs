use crate::app::{ConnectionManager, State, VpnApp};
use crate::cmd::{kill_openvpn, ProcessInfo};
use crate::config::Config;
use crate::dns::DnsResolver;
use crate::log::Log;
use crate::saml_server::{Saml, SamlServer};
use crate::task::OavcProcessTask;
use libc::{c_char, c_int, c_uint};
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
    pub vpn_app: Arc<VpnApp>,
    pub saml_server: SamlServer,
    pub callback: Option<extern "C" fn(status: VpnStatus, user_data: *mut libc::c_void)>,
    pub callback_data: *mut libc::c_void,
}

unsafe impl Send for VpnClientHandle {}
unsafe impl Sync for VpnClientHandle {}

/// Creates a new VPN client instance
#[no_mangle]
pub extern "C" fn openaws_vpn_client_new() -> *mut VpnClientHandle {
    let vpn_app = VpnApp::new();
    let saml_server = SamlServer::new();

    let client = Box::new(VpnClientHandle {
        vpn_app: vpn_app.clone(),
        saml_server,
        callback: None,
        callback_data: ptr::null_mut(),
    });

    let client_ptr = Box::into_raw(client);

    // Set up the callback with a static context
    if let Some(state_manager) = vpn_app.state.lock().unwrap().as_ref() {
        let client_ptr_clone = client_ptr as usize;
        state_manager.add_callback(move |state| {
            let client_ptr = client_ptr_clone as *mut VpnClientHandle;
            let client = unsafe { &*client_ptr };
            if let Some(callback) = client.callback {
                callback(VpnStatus::from(state), client.callback_data);
            }
        });
    }

    client_ptr
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

    // Call with current status
    if let Some(callback) = callback {
        if let Some(state_manager) = client.vpn_app.state.lock().unwrap().as_ref() {
            let current_state = state_manager.get_state();
            callback(VpnStatus::from(current_state), user_data);
        }
    }
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
    client
        .vpn_app
        .config
        .save_config(PathBuf::from(&config_path));

    // Update the remote server
    let mut remote = client.vpn_app.config.remote.lock().unwrap();
    *remote = Some((server_address, config.port as u16));

    // Resolve DNS addresses
    client.vpn_app.dns.resolve_addresses();

    0
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

    if let Some(state_manager) = client.vpn_app.state.lock().unwrap().as_ref() {
        VpnStatus::from(state_manager.get_state())
    } else {
        VpnStatus::Error
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
        let config = client.vpn_app.config.config.lock().unwrap();
        match &*config {
            Some(path) => path.clone(),
            None => return -1,
        }
    };

    let (server, port) = {
        let remote = client.vpn_app.config.remote.lock().unwrap();
        match &*remote {
            Some((addr, port)) => (addr.clone(), *port),
            None => return -1,
        }
    };

    // Set app status to connecting
    if let Some(state_manager) = client.vpn_app.state.lock().unwrap().as_ref() {
        state_manager.set_connecting();
    }

    // Run the initial OpenVPN process to get the SAML URL
    let auth_result = client.vpn_app.runtime.block_on(async {
        crate::cmd::run_ovpn(client.vpn_app.log.clone(), config_path, server, port).await
    });

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

/// Set up the SAML server
#[no_mangle]
pub extern "C" fn openaws_vpn_client_start_saml_server(client: *mut VpnClientHandle) -> c_int {
    let client = unsafe {
        if client.is_null() {
            return -1;
        }
        &mut *client
    };

    client.saml_server.start_server(client.vpn_app.clone());
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
    if let Some(state_manager) = client.vpn_app.state.lock().unwrap().as_ref() {
        state_manager.set_connecting();
    }

    // Get the config and remote settings
    let config_path = {
        let config = client.vpn_app.config.config.lock().unwrap();
        match &*config {
            Some(path) => path.clone(),
            None => return -1,
        }
    };

    let (server, port) = {
        let remote = client.vpn_app.config.remote.lock().unwrap();
        match &*remote {
            Some((addr, port)) => (addr.clone(), *port),
            None => return -1,
        }
    };

    let saml = Saml {
        data: saml_response,
        pwd: saml_password,
    };

    let process_info = Arc::new(ProcessInfo::new());
    let vpn_app = client.vpn_app.clone();
    let log_clone = vpn_app.log.clone();

    // Create clones for the async block
    let vpn_app_clone = vpn_app.clone();
    let process_info_clone = process_info.clone();

    // Start connection in the runtime
    let handle = vpn_app.runtime.spawn(async move {
        let result = crate::cmd::connect_ovpn(
            log_clone,
            config_path,
            server,
            port,
            saml,
            process_info_clone,
        )
        .await;

        // Update status based on result
        if let Some(state_manager) = vpn_app_clone.state.lock().unwrap().as_ref() {
            if result == 0 {
                state_manager.set_connected();
            } else {
                state_manager.set_disconnected();
            }
        }

        result
    });

    let task = OavcProcessTask::new(
        "OpenVPN Connection".to_string(),
        handle,
        client.vpn_app.log.clone(),
        process_info,
    );

    let mut connection = client.vpn_app.openvpn_connection.lock().unwrap();
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

    // Update status
    if let Some(state_manager) = client.vpn_app.state.lock().unwrap().as_ref() {
        state_manager.set_disconnected();
    }

    // Get connection manager
    let connection_manager = client.vpn_app.connection_manager.lock().unwrap();
    if let Some(manager) = connection_manager.as_ref() {
        if manager.disconnect() {
            return 0;
        }
    }

    -1
}

/// Frees resources used by the VPN client
#[no_mangle]
pub extern "C" fn openaws_vpn_client_free(client: *mut VpnClientHandle) {
    if !client.is_null() {
        let client = unsafe { Box::from_raw(client) };

        // Make sure to disconnect first
        let connection_manager = client.vpn_app.connection_manager.lock().unwrap();
        if let Some(manager) = connection_manager.as_ref() {
            manager.force_disconnect();
        }

        // Resources will be dropped when the Box is dropped
    }
}
