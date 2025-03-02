use crate::app::State;
use crate::app::VpnApp;
use crate::cmd::run_ovpn;
use crate::config::Pwd;
use crate::task::OavcTask;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

pub struct ConnectionManager {
    pub app: Arc<Mutex<Option<Arc<VpnApp>>>>,
}

unsafe impl Send for ConnectionManager {}
unsafe impl Sync for ConnectionManager {}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            app: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_app(&self, app: Arc<VpnApp>) {
        let mut l = self.app.lock().unwrap();
        *l = Some(app);
    }

    pub fn change_connect_state(&self) {
        let state = {
            let app_lock = self.app.lock().unwrap();
            let app = match app_lock.as_ref() {
                Some(app) => app.clone(),
                None => return,
            };
            let state_lock = app.state.lock().unwrap();
            match state_lock.as_ref() {
                Some(state_manager) => state_manager.get_state(),
                None => return,
            }
        };

        match state {
            State::Disconnected => self.connect(),
            State::Connected => self.disconnect(),
            State::Connecting => self.disconnect(),
        }
    }

    fn connect(&self) {
        println!("Connecting...");
        self.set_connecting();

        let app_lock = self.app.lock().unwrap();
        let app = match app_lock.as_ref() {
            Some(app) => app.clone(),
            None => return,
        };

        let file = app.config.config.lock().unwrap().deref().clone();
        let remote = app.config.remote.lock().unwrap().deref().clone();
        let addrs = app.config.addresses.lock().unwrap().deref().clone();

        if let Some(addrs) = addrs {
            if let Some(remote) = remote {
                if let Some(file) = file {
                    let log = app.log.clone();
                    let first_addr = addrs[0].to_string();
                    let config_file = file.clone();
                    let port = remote.1;
                    let pwd = app.config.pwd.clone();

                    let join = app.runtime.spawn(async move {
                        let mut lock = pwd.lock().await;
                        let auth = run_ovpn(log, config_file, first_addr, port).await;
                        *lock = Some(Pwd {
                            pwd: auth.pwd.clone(),
                        });
                        auth.url
                    });

                    let task = OavcTask {
                        name: "OpenVPN Initial SAML Process".to_string(),
                        handle: join,
                        log: app.log.clone(),
                    };

                    let mut openvpn = app.openvpn.lock().unwrap();
                    *openvpn = Some(task);
                }
                return;
            }
        }

        self.set_disconnected();
        app.log.append("No file selected");
    }

    pub fn disconnect(&self) {
        let app_lock = self.app.lock().unwrap();
        let app = match app_lock.as_ref() {
            Some(app) => app.clone(),
            None => return,
        };

        app.log.append("Disconnecting...");
        self.set_disconnected();

        let mut openvpn = app.openvpn.lock().unwrap();
        if let Some(ref srv) = openvpn.take() {
            srv.abort(true);
            app.log.append("OpenVPN Auth Disconnected!");
        }

        let mut openvpn_connection = app.openvpn_connection.lock().unwrap();
        if let Some(ref conn) = openvpn_connection.take() {
            conn.abort(true);
            app.log.append("OpenVPN disconnected!");
        }

        app.log.append("Disconnected!");
    }

    pub fn force_disconnect(&self) {
        println!("Forcing disconnect...");

        let app_lock = self.app.lock().unwrap();
        let app = match app_lock.as_ref() {
            Some(app) => app.clone(),
            None => return,
        };

        let mut openvpn = app.openvpn.lock().unwrap();
        if let Some(ref srv) = openvpn.take() {
            srv.abort(false);
        }

        let mut openvpn_connection = app.openvpn_connection.lock().unwrap();
        if let Some(ref conn) = openvpn_connection.take() {
            conn.abort(false);
        }
    }

    fn set_connecting(&self) {
        let app_lock = self.app.lock().unwrap();
        let app = match app_lock.as_ref() {
            Some(app) => app,
            None => return,
        };

        let state_lock = app.state.lock().unwrap();
        if let Some(state_manager) = state_lock.as_ref() {
            state_manager.set_connecting();
        }
    }

    fn set_disconnected(&self) {
        let app_lock = self.app.lock().unwrap();
        let app = match app_lock.as_ref() {
            Some(app) => app,
            None => return,
        };

        let state_lock = app.state.lock().unwrap();
        if let Some(state_manager) = state_lock.as_ref() {
            state_manager.set_disconnected();
        }
    }

    fn _set_connected(&self) {
        let app_lock = self.app.lock().unwrap();
        let app = match app_lock.as_ref() {
            Some(app) => app,
            None => return,
        };

        let state_lock = app.state.lock().unwrap();
        if let Some(state_manager) = state_lock.as_ref() {
            state_manager.set_connected();
        }
    }
}
