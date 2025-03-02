use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Log {
    messages: Arc<Mutex<Vec<String>>>,
    callbacks: Arc<Mutex<Vec<Box<dyn Fn(&str) + Send + Sync + 'static>>>>,
}

unsafe impl Send for Log {}
unsafe impl Sync for Log {}

impl Log {
    pub fn new() -> Log {
        Log {
            messages: Arc::new(Mutex::new(Vec::new())),
            callbacks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn add_callback<F>(&self, callback: F)
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.callbacks.lock().unwrap().push(Box::new(callback));
    }

    pub fn append<S: AsRef<str>>(&self, text: S) {
        let text_str = text.as_ref().to_string();

        // Store the message
        self.messages.lock().unwrap().push(text_str.clone());

        // Notify callbacks
        for callback in self.callbacks.lock().unwrap().iter() {
            callback(&text_str);
        }

        // Also print to console for debugging
        println!("[OpenAWS VPN] {}", text_str);
    }

    pub fn append_process(&self, pid: u32, text: &str) {
        let formatted = format!("[{}] {}", pid, text);
        self.append(formatted);
    }

    pub fn get_messages(&self) -> Vec<String> {
        self.messages.lock().unwrap().clone()
    }
}
