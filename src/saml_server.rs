use crate::app::VpnApp;
use crate::cmd::{connect_ovpn, ProcessInfo};
use crate::config::Pwd;
use crate::task::{OavcProcessTask, OavcTask};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;
use warp::http::StatusCode;
use warp::reply::WithStatus;
use warp::{Filter, Rejection};

pub struct SamlServer {
    // Add storage for SAML password
    saml_password: Arc<Mutex<String>>,
}

impl SamlServer {
    pub fn new() -> SamlServer {
        SamlServer {
            saml_password: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn start_server(&self, app: Arc<VpnApp>) {
        app.log.append("Starting SAML server at 0.0.0.0:35001...");
        let (tx, rx) = std::sync::mpsc::sync_channel::<Saml>(1);

        println!("Starting server");
        let sender = warp::any().map(move || tx.clone());

        let pwd = app.config.pwd.clone();
        let runtime = app.runtime.clone();
        let password_storage = self.saml_password.clone();

        // Save the password when we first get it from OpenVPN
        let _ = runtime.spawn(async move {
            let pwd_lock = pwd.lock().await;
            if let Some(ref pwd_val) = *pwd_lock {
                let mut password = password_storage.lock().unwrap();
                *password = pwd_val.pwd.clone();
                println!("Saved SAML password: {}", pwd_val.pwd);
            }
        });

        // Clone for the handler
        let password_for_handler = self.saml_password.clone();

        let saml = warp::post()
            .and(warp::body::form())
            .and(sender)
            .and(warp::any().map(move || password_for_handler.clone()))
            .and_then(
                move |data: HashMap<String, String>,
                      sender: SyncSender<Saml>,
                      password: Arc<Mutex<String>>| {
                    async move {
                        let saml_data = data.get("SAMLResponse").cloned().unwrap_or_default();

                        // Check if we have the SAMLResponse
                        if saml_data.is_empty() {
                            return Result::<WithStatus<_>, Rejection>::Ok(
                                warp::reply::with_status(
                                    "Error: Missing SAMLResponse field",
                                    StatusCode::BAD_REQUEST,
                                ),
                            );
                        }

                        // Get the stored password
                        let pwd_value = {
                            let p = password.lock().unwrap();
                            p.clone()
                        };

                        if pwd_value.is_empty() {
                            println!("WARNING: Password is empty!");
                        } else {
                            println!(
                                "Using password: {}",
                                if pwd_value.len() > 10 {
                                    format!("{}...", &pwd_value[..10])
                                } else {
                                    pwd_value.clone()
                                }
                            );
                        }

                        let saml = Saml {
                            data: saml_data,
                            pwd: pwd_value,
                        };

                        // Only send if we have valid data
                        if !saml.pwd.is_empty() {
                            sender.send(saml).unwrap_or_else(|e| {
                                println!("Failed to send SAML data: {:?}", e);
                            });
                            println!("Got SAML data with valid password!");
                        } else {
                            println!("Got SAML data but password was empty!");
                        }

                        Result::<WithStatus<_>, Rejection>::Ok(warp::reply::with_status(
                            "Got SAMLResponse field, it is now safe to close this window",
                            StatusCode::OK,
                        ))
                    }
                },
            );

        let handle = runtime.spawn(warp::serve(saml).run(([0, 0, 0, 0], 35001)));

        let log = app.log.clone();
        let join = OavcTask {
            name: "SAML Server".to_string(),
            handle,
            log,
        };

        let mut server = app.server.lock().unwrap();
        *server = Some(join);

        let log = app.log.clone();
        let addr = app.config.addresses.clone();
        let port = app.config.remote.clone();
        let config = app.config.config.clone();
        let st = app.openvpn_connection.clone();
        let stager = app.state.clone();
        let manager = app.connection_manager.clone();

        std::thread::spawn(move || loop {
            let data = rx.recv().unwrap();
            {
                log.append(format!("SAML Data received (length: {})", data.data.len()).as_str());
                log.append(
                    format!(
                        "SAML Password: {}",
                        if data.pwd.is_empty() {
                            "EMPTY!"
                        } else {
                            "VALID"
                        }
                    )
                    .as_str(),
                );
            }

            let addr = {
                let addr = addr.clone();
                let addr = addr.lock().unwrap();
                if let Some(addrs) = &*addr {
                    if !addrs.is_empty() {
                        addrs[0].to_string()
                    } else {
                        "".to_string()
                    }
                } else {
                    "".to_string()
                }
            };

            if addr.is_empty() {
                log.append("Error: No IP address available for connection");
                continue;
            }

            let config = {
                let config = config.clone();
                let config = config.lock().unwrap();
                match &*config {
                    Some(path) => path.clone(),
                    None => {
                        log.append("Error: No config file available");
                        continue;
                    }
                }
            };

            let port = {
                let port = port.clone();
                let port = port.lock().unwrap();
                match &*port {
                    Some((_addr, port)) => *port,
                    None => {
                        log.append("Error: No port available");
                        continue;
                    }
                }
            };

            let info = Arc::new(ProcessInfo::new());

            let handle = {
                let info = info.clone();
                let log = log.clone();
                let manager = manager.clone();
                log.append(
                    format!(
                        "Connecting to {} port {} with SAML authentication",
                        addr, port
                    )
                    .as_str(),
                );
                runtime.clone().spawn(async move {
                    let con = connect_ovpn(log.clone(), config, addr, port, data, info).await;
                    let man = manager.lock().unwrap();
                    if let Some(ref man) = *man {
                        man.disconnect();
                    }
                    con
                })
            };

            let task =
                OavcProcessTask::new("OpenVPN Connection".to_string(), handle, log.clone(), info);
            {
                let mut st = st.lock().unwrap();
                *st = Some(task);
            }

            let state_manager = stager.clone();
            let state_mgr = state_manager.lock().unwrap();
            if let Some(ref state_mgr) = *state_mgr {
                state_mgr.set_connected();
            }
        });
    }
}

#[derive(Debug, Clone)]
pub struct Saml {
    pub data: String,
    pub pwd: String,
}

unsafe impl Send for Saml {}
unsafe impl Sync for Saml {}
