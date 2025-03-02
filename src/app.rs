use crate::config::Config;
use crate::dns::DnsResolver;
use crate::log::Log;
use crate::state_manager::StateManager;
use crate::task::{OavcProcessTask, OavcTask};
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

// Simplified connection manager that doesn't rely on GTK
pub struct ConnectionManager {
    pub app: Arc<Mutex<Option<Arc<VpnApp>>>>,
}

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

    pub fn disconnect(&self) -> bool {
        let app = match self.app.lock().unwrap().as_ref() {
            Some(app) => app.clone(),
            None => return false,
        };

        app.log.append("Disconnecting...");

        let mut openvpn = app.openvpn.lock().unwrap();
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
        true
    }

    pub fn force_disconnect(&self) {
        println!("Forcing disconnect...");

        let app = match self.app.lock().unwrap().as_ref() {
            Some(app) => app.clone(),
            None => return,
        };

        let mut openvpn = app.openvpn.lock().unwrap();
        if let Some(ref srv) = openvpn.take() {
            srv.abort(false);
        }

        let openvpn_connection = app.openvpn_connection.clone();
        let mut openvpn_connection = openvpn_connection.lock().unwrap();
        if let Some(ref conn) = openvpn_connection.take() {
            conn.abort(false);
        }
    }
}

pub struct VpnApp {
    pub log: Arc<Log>,
    pub config: Arc<Config>,
    pub server: Arc<Mutex<Option<OavcTask<()>>>>,
    pub openvpn: Arc<Mutex<Option<OavcTask<String>>>>,
    pub openvpn_connection: Arc<Mutex<Option<OavcProcessTask<i32>>>>,
    pub runtime: Arc<Runtime>,
    pub dns: Arc<DnsResolver>,
    pub state: Arc<Mutex<Option<StateManager>>>,
    pub connection_manager: Arc<Mutex<Option<ConnectionManager>>>,
}

impl VpnApp {
    pub fn new() -> Arc<VpnApp> {
        let log = Arc::new(Log::new());
        let config = Arc::new(Config::new());
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        );

        let app = Arc::new(VpnApp {
            log: log.clone(),
            config: config.clone(),
            server: Arc::new(Mutex::new(None)),
            openvpn: Arc::new(Mutex::new(None)),
            openvpn_connection: Arc::new(Mutex::new(None)),
            runtime: runtime.clone(),
            dns: Arc::new(DnsResolver::new(
                config.clone(),
                log.clone(),
                runtime.clone(),
            )),
            state: Arc::new(Mutex::new(None)),
            connection_manager: Arc::new(Mutex::new(None)),
        });

        // Initialize default state manager
        app.setup_state_manager();

        // Setup connection manager
        let connection_manager = ConnectionManager::new();
        connection_manager.set_app(app.clone());
        app.set_connection_manager(connection_manager);

        app
    }

    pub fn setup_state_manager(&self) {
        let mut b = self.state.lock().unwrap();
        *b = Some(StateManager::new(self.log.clone()));
    }

    pub fn set_connection_manager(&self, manager: ConnectionManager) {
        let mut current = self.connection_manager.lock().unwrap();
        *current = Some(manager);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum State {
    Connecting,
    Connected,
    Disconnected,
}
