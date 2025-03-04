mod app;
mod cmd;
mod config;
mod consts;
mod dns;
mod local_config;
mod log;
mod manager;
mod saml_server;
mod state_manager;
mod task;

use crate::app::{State, VpnApp};
use crate::cmd::kill_openvpn;
use crate::local_config::LocalConfig;
use crate::manager::ConnectionManager;
use crate::saml_server::SamlServer;
use clap::{App, Arg, SubCommand};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

fn main() {
    // Command line argument parsing
    let matches = App::new("OpenAwsVpnClient")
        .version("1.0")
        .author("Your Name <your.email@example.com>")
        .about("Command line AWS VPN client")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Sets the OVPN config file to use")
                .takes_value(true),
        )
        .subcommand(
            SubCommand::with_name("connect")
                .about("Connect to VPN using specified config")
                .arg(
                    Arg::with_name("config")
                        .short("c")
                        .long("config")
                        .value_name("FILE")
                        .help("Sets the OVPN config file to use")
                        .takes_value(true)
                        .required(true),
                ),
        )
        .subcommand(SubCommand::with_name("disconnect").about("Disconnect from VPN"))
        .subcommand(SubCommand::with_name("status").about("Show connection status"))
        .get_matches();

    // Initialize the VPN application
    let vpn_app = Rc::new(VpnApp::new());

    // Setup connection manager
    let connection_manager = ConnectionManager::new();
    connection_manager.set_app(vpn_app.clone());
    vpn_app.set_connection_manager(connection_manager);

    // Start SAML server
    let saml_server = SamlServer::new();
    saml_server.start_server(vpn_app.clone());

    // Check for any lingering OpenVPN sessions
    if let Some(p) = LocalConfig::read_last_pid() {
        println!("Last OpenVPN session (PID: {}) was not closed properly", p);
        println!("Killing it in 5 seconds...");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            kill_openvpn(p);
        });
    }

    // Handle command-line arguments
    if let Some(matches) = matches.subcommand_matches("connect") {
        if let Some(config_file) = matches.value_of("config") {
            let config_path = PathBuf::from(config_file);
            if config_path.exists() {
                println!("Connecting using config: {}", config_file);
                vpn_app.config.save_config(&config_path);
                vpn_app.dns.resolve_addresses();

                // Get the connection manager and initiate connection
                let manager = vpn_app.connection_manager.lock().unwrap();
                if let Some(ref manager) = *manager {
                    manager.connect();

                    // Wait for connection to complete or fail
                    let mut attempts = 0;
                    loop {
                        let state = {
                            let app = vpn_app.clone();
                            let state_manager = app.state.lock().unwrap();
                            if let Some(ref state_manager) = *state_manager {
                                *state_manager.state.borrow()
                            } else {
                                break;
                            }
                        };

                        match state {
                            State::Connected => {
                                println!("Successfully connected to VPN");

                                loop {
                                    std::thread::sleep(Duration::from_secs(1));
                                }

                                //break;
                            }
                            State::Disconnected => {
                                if attempts > 0 {
                                    // If we've looped at least once
                                    println!("Failed to connect to VPN");
                                    break;
                                }
                            }
                            _ => {}
                        }

                        attempts += 1;
                        if attempts > 120 {
                            // Wait up to 2 minutes (120 seconds)
                            println!("Connection timed out");
                            break;
                        }
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }
            } else {
                eprintln!("Config file does not exist: {}", config_file);
            }
        }
    } else if let Some(_) = matches.subcommand_matches("disconnect") {
        println!("Disconnecting from VPN...");
        let manager = vpn_app.connection_manager.lock().unwrap();
        if let Some(ref manager) = *manager {
            manager.disconnect();
            println!("Disconnected");
        }
    } else if let Some(_) = matches.subcommand_matches("status") {
        let state = {
            let app = vpn_app.clone();
            let state_manager = app.state.lock().unwrap();
            if let Some(ref state_manager) = *state_manager {
                *state_manager.state.borrow()
            } else {
                State::Disconnected
            }
        };

        match state {
            State::Connected => println!("Status: Connected"),
            State::Connecting => println!("Status: Connecting"),
            State::Disconnected => println!("Status: Disconnected"),
        }
    } else if let Some(config_file) = matches.value_of("config") {
        // If only config is provided with no subcommand, treat it like connect
        let config_path = PathBuf::from(config_file);
        if config_path.exists() {
            println!("Connecting using config: {}", config_file);
            vpn_app.config.save_config(&config_path);
            vpn_app.dns.resolve_addresses();

            let manager = vpn_app.connection_manager.lock().unwrap();
            if let Some(ref manager) = *manager {
                manager.connect();

                // Keep the main thread alive to maintain connection
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        } else {
            eprintln!("Config file does not exist: {}", config_file);
        }
    } else {
        println!("No command specified. Use --help for usage information.");
    }

    // Clean up when exiting
    let manager = vpn_app.connection_manager.lock().unwrap();
    if let Some(manager) = manager.as_ref() {
        manager.force_disconnect();
    }
}
