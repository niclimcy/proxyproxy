use super::intent::{OnboardingIntent, OnboardingTarget};
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
            cfg,
        }
    }

    pub fn open(&mut self) {
        self.show = true;
        self.stage = OnboardingStage::ChooseExperience;
    }

    pub fn handle(&mut self, intent: OnboardingIntent) {
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
            OnboardingIntent::Skip | OnboardingIntent::Complete => self.complete(),
            OnboardingIntent::Device(_) | OnboardingIntent::ImportFlows(_) => {}
        }
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
    fn ios_selection_is_persisted() {
        let mut vm = rig(false);
        vm.handle(OnboardingIntent::SelectIosSimulator("UDID-A".into()));
        assert_eq!(vm.selected_ios_udid.as_deref(), Some("UDID-A"));
    }

    #[test]
    fn reopen_and_complete_roundtrip() {
        let mut vm = rig(true);
        vm.open();
        assert!(vm.show);
        vm.handle(OnboardingIntent::Complete);
        assert!(!vm.show);
    }
}
