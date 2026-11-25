#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsTab {
    Network,
    Schemas,
    Shortcuts,
}

pub struct SettingsVm {
    pub show: bool,
    pub tab: SettingsTab,
}

impl Default for SettingsVm {
    fn default() -> Self {
        Self {
            show: false,
            tab: SettingsTab::Network,
        }
    }
}

impl SettingsVm {
    pub fn open(&mut self) {
        self.show = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_shows_window_and_keeps_tab() {
        let mut vm = SettingsVm::default();
        assert!(!vm.show);
        vm.tab = SettingsTab::Schemas;
        vm.open();
        assert!(vm.show);
        assert_eq!(vm.tab, SettingsTab::Schemas);
    }
}
