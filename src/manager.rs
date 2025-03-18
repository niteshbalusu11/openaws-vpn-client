use crate::app::{State, VpnApp};
use crate::cmd::run_ovpn;
use crate::config::Pwd;
use crate::task::OavcTask;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::sync::Mutex;

pub struct ConnectionManager {
    pub app: Mutex<Weak<VpnApp>>,
}

unsafe impl Send for ConnectionManager {}
unsafe impl Sync for ConnectionManager {}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            app: Mutex::new(Weak::new()),
        }
    }

    pub fn set_app(&self, app: Rc<VpnApp>) {
        let mut l = self.app.lock().unwrap();
        *l = Rc::downgrade(&app);
    }

    pub fn change_connect_state(&self) {
        let state = {
            let app = self.app.lock().unwrap();
            let app = app.upgrade().unwrap();
            let state = { *(app.state.lock().unwrap()).as_ref().unwrap().state.borrow() };
            app.log.append(format!("Handling... {:?}", &state));
            state
        };

        match state {
            State::Disconnected => self.connect(),
            State::Connected => self.disconnect(),
            State::Connecting => self.try_disconnect(),
        }
    }

    pub fn try_disconnect(&self) {
        let state = {
            let app = self.app.lock().unwrap();
            let app = app.upgrade().unwrap();
            let state = { *(app.state.lock().unwrap()).as_ref().unwrap().state.borrow() };
            app.log.append(format!("Handling... {:?}", &state));
            state
        };

        match state {
            State::Disconnected => (),
            _ => self.disconnect(),
        }
    }

    pub fn connect(&self) {
        println!("Connecting...");
        self.set_connecting();

        let (file, remote, addrs) = {
            let app = self.app.lock().unwrap();
            let app = app.upgrade().unwrap();

            (
                {
                    let x = app.config.config.lock().unwrap().deref().clone();
                    x
                },
                {
                    let x = app.config.remote.lock().unwrap().deref().clone();
                    x
                },
                {
                    let x = app.config.addresses.lock().unwrap().deref().clone();
                    x
                },
            )
        };

        // Check if addresses are available first
        if let Some(ref addrs) = addrs {
            if addrs.is_empty() {
                // Handle the case where DNS resolution failed to find any addresses
                let app = self.app.lock().unwrap();
                let app = app.upgrade().unwrap();
                app.log
                    .append("Error: DNS resolution failed - no addresses found for VPN endpoint");
                self.set_disconnected();
                return;
            }

            if let Some(ref remote) = remote {
                if let Some(ref file) = file {
                    let log = {
                        let app = self.app.lock().unwrap();
                        let app = app.upgrade().unwrap();
                        app.log.clone()
                    };

                    let first_addr = addrs[0].to_string(); // Now safe because we checked if it's empty
                    let config_file = file.clone();
                    let port = remote.1;

                    let pwd = {
                        let app = self.app.lock().unwrap();
                        let app = app.upgrade().unwrap();
                        app.config.pwd.clone()
                    };

                    let join = {
                        let app = self.app.lock().unwrap();
                        let app = app.upgrade().unwrap();
                        let log = log.clone();

                        app.runtime.spawn(async move {
                            let mut lock = pwd.lock().await;
                            let auth = run_ovpn(log, config_file, first_addr, port).await;
                            *lock = Some(Pwd { pwd: auth.pwd });

                            println!("Please authenticate in your browser: {}", auth.url);
                            open::that(auth.url).unwrap()
                        })
                    };

                    let log = log.clone();
                    let app = self.app.lock().unwrap();
                    let app = app.upgrade().unwrap();
                    app.openvpn.replace(Some(OavcTask {
                        name: "OpenVPN Initial SAML Process".to_string(),
                        handle: join,
                        log,
                    }));
                    return;
                }
            }
        }

        // If we get here, something is missing (file, remote, or addresses)
        let app = self.app.lock().unwrap();
        let app = app.upgrade().unwrap();

        if addrs.is_none() || addrs.as_ref().unwrap().is_empty() {
            app.log
                .append("Error: DNS resolution failed for VPN endpoint");
        } else if remote.is_none() {
            app.log.append("Error: Remote configuration missing");
        } else if file.is_none() {
            app.log.append("Error: No VPN configuration file selected");
        } else {
            app.log.append("Error: Unknown connection issue");
        }

        self.set_disconnected();
    }
    pub fn force_disconnect(&self) {
        println!("Forcing disconnect...");

        let app = self.app.lock().unwrap();
        let app = app.upgrade().unwrap();
        let mut openvpn = app.openvpn.borrow_mut();

        if let Some(ref srv) = openvpn.take() {
            srv.abort(false);
        }

        let openvpn_connection = app.openvpn_connection.clone();
        let mut openvpn_connection = openvpn_connection.lock().unwrap();
        if let Some(ref conn) = openvpn_connection.take() {
            conn.abort(false);
        }
    }

    pub fn disconnect(&self) {
        {
            let app = self.app.lock().unwrap();
            let app = app.upgrade().unwrap();

            app.log.append("Disconnecting...");
        }

        self.set_disconnected();

        {
            let app = self.app.lock().unwrap();
            let app = app.upgrade().unwrap();

            let mut openvpn = app.openvpn.borrow_mut();

            if let Some(ref srv) = openvpn.take() {
                srv.abort(true);
                app.log.append("OpenVPN Auth Disconnected!");
            }

            let openvpn_connection = app.openvpn_connection.clone();
            let mut openvpn_connection = openvpn_connection.lock().unwrap();
            if let Some(ref conn) = openvpn_connection.take() {
                conn.abort(true);
                app.log.append("OpenVPN disconnected!");
            }

            app.log.append("Disconnected!");
        }
    }

    fn set_connecting(&self) {
        let app = self.app.lock().unwrap();
        let app = app.upgrade().unwrap();
        app.state.lock().unwrap().as_ref().unwrap().set_connecting();
    }

    fn set_disconnected(&self) {
        let app = self.app.lock().unwrap();
        let app = app.upgrade().unwrap();
        app.state
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .set_disconnected();
    }

    fn _set_connected(&self) {
        let app = self.app.lock().unwrap();
        let app = app.upgrade().unwrap();
        app.state.lock().unwrap().as_ref().unwrap().set_connected();
    }
}
