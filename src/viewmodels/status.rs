use crate::domain::repaint::Repaint;
use std::sync::{
    Arc,
    mpsc::{Receiver, Sender, channel},
};

#[derive(Clone)]
pub struct JobRunner {
    tx: Sender<String>,
    repaint: Arc<dyn Repaint>,
}

impl JobRunner {
    pub fn spawn(&self, job: impl FnOnce() -> Result<String, String> + Send + 'static) {
        let (tx, repaint) = (self.tx.clone(), self.repaint.clone());
        std::thread::spawn(move || {
            let _ = tx.send(job().unwrap_or_else(|e| format!("Error: {e}")));
            repaint.request();
        });
    }
}

pub struct Status {
    pub text: String,
    pub jobs: JobRunner,
    rx: Receiver<String>,
}

impl Status {
    pub fn new(text: String, repaint: Arc<dyn Repaint>) -> Self {
        let (tx, rx) = channel();
        Self { text, jobs: JobRunner { tx, repaint }, rx }
    }

    pub fn set(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn poll(&mut self) {
        while let Ok(n) = self.rx.try_recv() {
            self.text = n;
        }
    }
}

#[cfg(test)]
pub struct NoRepaint;

#[cfg(test)]
impl Repaint for NoRepaint {
    fn request(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_result_lands_in_status_after_poll() {
        let mut s = Status::new("idle".into(), Arc::new(NoRepaint));
        s.jobs.spawn(|| Ok("done".into()));
        for _ in 0..100 {
            s.poll();
            if s.text != "idle" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(s.text, "done");
    }

    #[test]
    fn job_error_is_prefixed() {
        let mut s = Status::new(String::new(), Arc::new(NoRepaint));
        s.jobs.spawn(|| Err("nope".into()));
        for _ in 0..100 {
            s.poll();
            if !s.text.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(s.text, "Error: nope");
    }
}
