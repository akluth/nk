pub struct Message {
    pub from: &'static str,
    pub to: &'static str,
    pub body: &'static str,
}

impl Message {
    pub const fn new(from: &'static str, to: &'static str, body: &'static str) -> Self {
        Self { from, to, body }
    }
}

pub struct MessageBus {
    last: Option<Message>,
}

impl MessageBus {
    pub const fn new() -> Self {
        Self { last: None }
    }

    pub fn publish(&mut self, message: Message) {
        let _route = (message.from, message.to, message.body);
        self.last = Some(message);
    }
}
