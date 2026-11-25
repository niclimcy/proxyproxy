#[derive(Default)]
pub struct AboutVm {
    pub show: bool,
}

impl AboutVm {
    pub fn open(&mut self) {
        self.show = true;
    }
}
