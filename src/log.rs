use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Log {
    view: Arc<View>,
}

pub struct View {
    buffer: Arc<Mutex<Vec<String>>>,
}

unsafe impl Send for Log {}
unsafe impl Sync for Log {}

unsafe impl Send for View {}
unsafe impl Sync for View {}

impl Log {
    pub fn new() -> Log {
        Log {
            view: Arc::new(View {
                buffer: Arc::new(Mutex::new(Vec::new())),
            }),
        }
    }

    pub fn append<S: AsRef<str>>(&self, text: S) {
        let text = text.as_ref().to_string();
        println!("{}", text);

        let mut buffer = self.view.buffer.lock().unwrap();
        buffer.push(text.clone());
    }

    pub fn append_process(&self, pid: u32, text: &str) {
        let text = format!("[{}] {}", pid, text);
        println!("{}", text);

        let mut buffer = self.view.buffer.lock().unwrap();
        buffer.push(text.clone());
    }

    // New method to get the last log message with a callback
    pub fn get_last_log<F>(&self, callback: Box<F>)
    where
        F: FnOnce(&str) + Send + 'static,
    {
        let buffer = self.view.buffer.lock().unwrap();
        if let Some(last) = buffer.last() {
            callback(last);
        } else {
            callback("");
        }
    }

    // New method to get all log messages
    pub fn get_all_logs<F>(&self, callback: Box<F>)
    where
        F: FnOnce(&[String]) + Send + 'static,
    {
        let buffer = self.view.buffer.lock().unwrap();
        callback(&buffer);
    }

    // New method to clear logs
    pub fn clear(&self) {
        let mut buffer = self.view.buffer.lock().unwrap();
        buffer.clear();
    }
}
