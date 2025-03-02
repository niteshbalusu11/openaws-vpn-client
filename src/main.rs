use std::ffi::CString;
use std::io::{self, BufRead};
use std::ptr;

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

extern "C" fn status_callback(status: ffi::VpnStatus, _user_data: *mut libc::c_void) {
    match status {
        ffi::VpnStatus::Disconnected => println!("VPN Status: DISCONNECTED"),
        ffi::VpnStatus::Connecting => println!("VPN Status: CONNECTING"),
        ffi::VpnStatus::Connected => println!("VPN Status: CONNECTED"),
        ffi::VpnStatus::Error => println!("VPN Status: ERROR"),
    }
}

fn main() {
    println!("OpenAWS VPN Client Test");

    // Get OpenVPN config file path
    let mut config_path = String::new();
    println!("Enter path to .ovpn config file:");
    //io::stdin()
    //    .lock()
    //    .read_line(&mut config_path)
    //    .expect("Failed to read config path");
    config_path = "/Users/niteshchowdharybalusu/Downloads/zbdvpn.ovpn".to_string();

    // Create VPN client
    let client = ffi::openaws_vpn_client_new();
    if client.is_null() {
        println!("Failed to create VPN client");
        return;
    }

    // Set status callback
    ffi::openaws_vpn_client_set_status_callback(client, Some(status_callback), ptr::null_mut());

    println!("Setting configuration...");
    // Set configuration
    let config_cstring = CString::new(config_path).unwrap();
    let dummy_cstring = CString::new("").unwrap();

    let config = ffi::VpnConfig {
        config_path: config_cstring.as_ptr(),
        server_address: dummy_cstring.as_ptr(), // Will be read from config file
        port: 0,                                // Will be read from config file
    };

    println!("Config path: {:?}", config.config_path);

    let result = ffi::openaws_vpn_client_set_config(client, config);

    if result != 0 {
        println!("Failed to set configuration");
        ffi::openaws_vpn_client_free(client);
        return;
    }

    println!("Configuration set");

    // Start SAML server
    let result = ffi::openaws_vpn_client_start_saml_server(client);
    if result != 0 {
        println!("Failed to start SAML server");
        ffi::openaws_vpn_client_free(client);
        return;
    }

    println!("Starting DNS resolution...");

    // Get SAML authentication URL
    let mut saml_url: *mut libc::c_char = ptr::null_mut();
    let mut saml_password: *mut libc::c_char = ptr::null_mut();

    println!("Attempting to get SAML URL (this may take a while)...");
    let result = ffi::openaws_vpn_client_get_saml_url(client, &mut saml_url, &mut saml_password);

    if result != 0 || saml_url.is_null() || saml_password.is_null() {
        println!("Failed to get SAML authentication URL");
        ffi::openaws_vpn_client_free(client);
        return;
    }

    // Convert C strings to Rust strings
    let url = unsafe { CString::from_raw(saml_url).into_string().unwrap() };
    let password = unsafe { CString::from_raw(saml_password).into_string().unwrap() };

    println!("\nPlease visit this URL in your browser to authenticate:");
    println!("{}", url);
    println!("\nPassword for SAML authentication: {}", password);

    // Wait for SAML response
    println!("\nAfter authentication, paste the SAMLResponse value here:");
    let mut saml_response = String::new();
    io::stdin()
        .lock()
        .read_line(&mut saml_response)
        .expect("Failed to read SAML response");
    saml_response = saml_response.trim().to_string();

    // Connect with SAML response
    let saml_response_cstring = CString::new(saml_response).unwrap();
    let saml_password_cstring = CString::new(password).unwrap();

    let result = ffi::openaws_vpn_client_connect_saml(
        client,
        saml_response_cstring.as_ptr(),
        saml_password_cstring.as_ptr(),
    );

    if result != 0 {
        println!("Failed to connect to VPN");
        ffi::openaws_vpn_client_free(client);
        return;
    }

    println!("VPN connection initiated.");
    println!("Press Enter to disconnect...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    // Disconnect
    ffi::openaws_vpn_client_disconnect(client);

    // Clean up
    ffi::openaws_vpn_client_free(client);

    println!("VPN disconnected");
}
