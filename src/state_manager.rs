use crate::app::State;
use crate::consts::*;
use crate::log::Log;
use std::cell::RefCell;
use std::sync::Arc;

pub struct StateManager {
    pub log: Arc<Log>,
    pub state: RefCell<State>,
    pub callbacks: RefCell<Vec<Box<dyn Fn(State) + Send + Sync + 'static>>>,
}

unsafe impl Send for StateManager {}
unsafe impl Sync for StateManager {}

impl StateManager {
    pub fn new(log: Arc<Log>) -> StateManager {
        let manager = StateManager {
            log,
            state: RefCell::new(State::Disconnected),
            callbacks: RefCell::new(Vec::new()),
        };
        return manager;
    }

    pub fn add_callback<F>(&self, callback: F)
    where
        F: Fn(State) + Send + Sync + 'static,
    {
        self.callbacks.borrow_mut().push(Box::new(callback));
    }

    fn notify_state_change(&self, state: State) {
        for callback in self.callbacks.borrow().iter() {
            callback(state);
        }
    }

    pub fn set_connecting(&self) {
        self.state.replace(State::Connecting);
        self.log.append(CONNECTING);
        self.notify_state_change(State::Connecting);
    }

    pub fn set_disconnected(&self) {
        self.state.replace(State::Disconnected);
        self.log.append(DISCONNECTED);
        self.notify_state_change(State::Disconnected);
    }

    pub fn set_connected(&self) {
        self.state.replace(State::Connected);
        self.log.append(CONNECTED);
        self.notify_state_change(State::Connected);
    }

    pub fn get_state(&self) -> State {
        *self.state.borrow()
    }
}

// Don't derive Clone for StateManager since it contains callbacks
// that can't be easily cloned
