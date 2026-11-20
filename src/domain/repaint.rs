pub trait Repaint: Send + Sync {
    fn request(&self);
}
