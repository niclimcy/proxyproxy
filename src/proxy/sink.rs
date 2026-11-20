use crate::domain::flow::Event;
use crate::domain::repaint::Repaint;
use std::sync::{Arc, mpsc::Sender};

pub trait FlowSink: Send + Sync {
    fn emit(&self, ev: Event);
}

pub struct ChannelSink {
    tx: Sender<Event>,
    repaint: Arc<dyn Repaint>,
}

impl ChannelSink {
    pub fn new(tx: Sender<Event>, repaint: Arc<dyn Repaint>) -> Self {
        Self { tx, repaint }
    }
}

impl FlowSink for ChannelSink {
    fn emit(&self, ev: Event) {
        let _ = self.tx.send(ev);
        self.repaint.request();
    }
}
