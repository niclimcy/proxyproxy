use super::intent::{Effect, OnboardingIntent, OnboardingTarget};
use super::status::Flash;
use crate::domain::config::{OnboardingProfile, Shared};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OnboardingStage {
    ChooseExperience,
    ChooseTarget,
    Beginner(OnboardingTarget),
    Experienced,
}

pub struct OnboardingVm {
    pub show: bool,
    pub stage: OnboardingStage,
    pub profile: OnboardingProfile,
    pub selected_ios_udid: Option<String>,
    pub env_copied: Flash,
    cfg: Shared,
}

impl OnboardingVm {
    pub fn new(cfg: Shared) -> Self {
        let (completed, profile) = {
            let cfg = cfg.lock().unwrap();
            (cfg.onboarding_completed, cfg.onboarding_profile)
        };
        Self {
            show: !completed,
            stage: OnboardingStage::ChooseExperience,
            profile,
            selected_ios_udid: None,
            env_copied: Flash::default(),
            cfg,
        }
    }

    pub fn open(&mut self) {
        self.show = true;
        self.stage = OnboardingStage::ChooseExperience;
    }

    pub fn handle(&mut self, intent: OnboardingIntent) -> Option<Effect> {
        match intent {
            OnboardingIntent::ChooseProfile(profile) => {
                self.profile = profile;
                self.stage = match profile {
                    OnboardingProfile::Beginner => OnboardingStage::ChooseTarget,
                    OnboardingProfile::Experienced => OnboardingStage::Experienced,
                };
            }
            OnboardingIntent::ChooseTarget(target) => {
                self.stage = OnboardingStage::Beginner(target);
            }
            OnboardingIntent::SelectIosSimulator(udid) => {
                self.selected_ios_udid = Some(udid);
            }
            OnboardingIntent::Back => {
                self.stage = match self.stage {
                    OnboardingStage::Beginner(_) => OnboardingStage::ChooseTarget,
                    OnboardingStage::ChooseTarget | OnboardingStage::Experienced => {
                        OnboardingStage::ChooseExperience
                    }
                    OnboardingStage::ChooseExperience => OnboardingStage::ChooseExperience,
                };
            }
            OnboardingIntent::EnvCopied => self.env_copied.mark(),
            OnboardingIntent::Finish => self.complete(),
            OnboardingIntent::Device(action) => return Some(Effect::Device(action)),
            OnboardingIntent::ImportFlows(path) => return Some(Effect::ImportFlows(path)),
        }
        None
    }

    fn complete(&mut self) {
        {
            let mut cfg = self.cfg.lock().unwrap();
            cfg.onboarding_completed = true;
            cfg.onboarding_profile = self.profile;
        }
        self.show = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::Config;
    use std::sync::{Arc, Mutex};

    fn rig(completed: bool) -> OnboardingVm {
        let cfg = Arc::new(Mutex::new(Config {
            onboarding_completed: completed,
            ..Config::default()
        }));
        OnboardingVm::new(cfg)
    }

    #[test]
    fn first_run_shows_experience_choice() {
        let vm = rig(false);
        assert!(vm.show);
        assert_eq!(vm.stage, OnboardingStage::ChooseExperience);
    }

    #[test]
    fn completed_run_stays_closed() {
        let vm = rig(true);
        assert!(!vm.show);
    }

    #[test]
    fn beginner_flow_has_target_then_platform_setup() {
        let mut vm = rig(false);
        vm.handle(OnboardingIntent::ChooseProfile(OnboardingProfile::Beginner));
        assert_eq!(vm.stage, OnboardingStage::ChooseTarget);
        vm.handle(OnboardingIntent::ChooseTarget(OnboardingTarget::Android));
        assert_eq!(
            vm.stage,
            OnboardingStage::Beginner(OnboardingTarget::Android)
        );
        vm.handle(OnboardingIntent::Back);
        assert_eq!(vm.stage, OnboardingStage::ChooseTarget);
    }

    #[test]
    fn experienced_flow_opens_compact_actions() {
        let mut vm = rig(false);
        vm.handle(OnboardingIntent::ChooseProfile(
            OnboardingProfile::Experienced,
        ));
        assert_eq!(vm.stage, OnboardingStage::Experienced);
    }

    #[test]
    fn ios_selection_is_remembered() {
        let mut vm = rig(false);
        vm.handle(OnboardingIntent::SelectIosSimulator("UDID-A".into()));
        assert_eq!(vm.selected_ios_udid.as_deref(), Some("UDID-A"));
    }

    #[test]
    fn reopen_and_complete_roundtrip() {
        let mut vm = rig(true);
        vm.open();
        assert!(vm.show);
        assert!(vm.handle(OnboardingIntent::Finish).is_none());
        assert!(!vm.show);
        assert!(vm.cfg.lock().unwrap().onboarding_completed);
    }

    #[test]
    fn device_and_import_intents_become_effects() {
        let mut vm = rig(false);
        assert!(matches!(
            vm.handle(OnboardingIntent::Device(
                crate::viewmodels::intent::DeviceAction::TrustCa
            )),
            Some(Effect::Device(_))
        ));
        assert!(matches!(
            vm.handle(OnboardingIntent::ImportFlows("a.har".into())),
            Some(Effect::ImportFlows(_))
        ));
    }
}
