use crate::consts::*;
use crate::log::Log;
use crate::State;
use std::cell::RefCell;
use std::sync::Arc;

#[derive(Clone)]
pub struct StateManager {
    pub log: Arc<Log>,
    pub state: RefCell<State>,
}

unsafe impl Send for StateManager {}
unsafe impl Sync for StateManager {}

impl StateManager {
    pub fn new(log: Arc<Log>) -> StateManager {
        let manager = StateManager {
            log,
            state: RefCell::new(State::Disconnected),
        };
        return manager;
    }
}

impl StateManager {
    pub fn set_connecting(&self) {
        self.state.replace(State::Connecting);
        self.log.append(CONNECTING);
    }

    pub fn set_disconnected(&self) {
        self.state.replace(State::Disconnected);
        self.log.append(DISCONNECTED);
    }

    pub fn set_connected(&self) {
        self.state.replace(State::Connected);
        self.log.append(CONNECTED);
    }
}
