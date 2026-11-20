#[derive(Default)]
pub struct AboutVm {
    pub show: bool,
}

impl AboutVm {
    pub fn open(&mut self) {
        self.show = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_shows_window() {
        let mut vm = AboutVm::default();
        assert!(!vm.show);
        vm.open();
        assert!(vm.show);
    }
}
