use std::sync::mpsc;
use std::sync::Arc;

pub enum EventBody {
    Log(String),
    Dead,
}
pub struct Event {
    pub sender: String,
    pub body: EventBody,
}

#[derive(Clone)]
pub struct EventBusHandle {
    pub sender: Arc<mpsc::Sender<Event>>,
    pub task_name: String,
}
impl EventBusHandle {
    pub fn log(&self, msg: String) {
        // If it fails, don't panic
        _ = self.sender.send(Event {
            sender: self.task_name.clone(),
            body: EventBody::Log(msg),
        });
    }

    pub fn fatal_error(&self, msg: String) {
        // If it fails, don't panic
        _ = self.sender.send(Event {
            sender: self.task_name.clone(),
            body: EventBody::Log(format!("FATAL ERROR: {}", msg)),
        });
        _ = self.sender.send(Event {
            sender: self.task_name.clone(),
            body: EventBody::Dead,
        });
    }
    pub fn announce_dead(&self) {
        _ = self.sender.send(Event {
            sender: self.task_name.clone(),
            body: EventBody::Dead,
        });
    }
}
