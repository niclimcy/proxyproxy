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
