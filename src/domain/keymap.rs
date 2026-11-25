use super::config::Config;
use serde::{Deserialize, Serialize};

/// A row menu action that can also run from the keyboard on the selected flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowAction {
    Replay,
    Pin,
    Color(Option<u8>),
    Comment,
    DiffBase,
    Mute,
    Delete,
}

/// Cmd (Ctrl off macOS) plus `key`, named as the UI toolkit names keys ("P", "1", "Slash"), and Shift when `shift` is set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub key: String,
    pub shift: bool,
}

impl Binding {
    fn new(key: &str, shift: bool) -> Self {
        Self {
            key: key.into(),
            shift,
        }
    }
}

/// Actions with a Cmd shortcut, in the order Settings lists them. Delete stays on the Delete key.
pub const CUSTOMIZABLE: [RowAction; 11] = [
    RowAction::Replay,
    RowAction::Pin,
    RowAction::Comment,
    RowAction::DiffBase,
    RowAction::Mute,
    RowAction::Color(Some(0)),
    RowAction::Color(Some(1)),
    RowAction::Color(Some(2)),
    RowAction::Color(Some(3)),
    RowAction::Color(Some(4)),
    RowAction::Color(None),
];

pub fn default_binding(action: RowAction) -> Option<Binding> {
    Some(match action {
        RowAction::Replay => Binding::new("R", false),
        RowAction::Pin => Binding::new("P", false),
        RowAction::Comment => Binding::new("Slash", false),
        RowAction::DiffBase => Binding::new("D", false),
        // Cmd+M is Minimize on macOS
        RowAction::Mute => Binding::new("M", true),
        RowAction::Color(Some(i)) => Binding::new(&(i + 1).to_string(), false),
        RowAction::Color(None) => Binding::new("0", false),
        RowAction::Delete => return None,
    })
}

/// Cmd combos the app, the UI zoom or the OS menu already use: key name and Shift.
const RESERVED: [(&str, bool); 17] = [
    ("B", false),
    ("F", false),
    ("Comma", false),
    ("P", true),
    ("W", false),
    ("M", false),
    ("H", false),
    ("Q", false),
    ("A", false),
    ("C", false),
    ("V", false),
    ("X", false),
    ("Z", false),
    ("Z", true),
    ("Plus", false),
    ("Minus", false),
    ("Equals", false),
];

/// Named keys that can follow Cmd; letters and digits are allowed too.
const PUNCTUATION: [&str; 8] = [
    "Slash",
    "Backslash",
    "Period",
    "Semicolon",
    "Quote",
    "Backtick",
    "OpenBracket",
    "CloseBracket",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BindError {
    /// Not a letter, digit or punctuation key.
    Unsupported,
    /// Used by the app, the UI zoom or the OS menu.
    Reserved,
    InUse(RowAction),
}

impl Config {
    /// The shortcut in effect for `action`, or `None` when it has none.
    pub fn binding(&self, action: RowAction) -> Option<Binding> {
        match self.shortcuts.iter().find(|(a, _)| *a == action) {
            Some((_, b)) => b.clone(),
            None => default_binding(action),
        }
    }

    pub fn keymap(&self) -> Vec<(RowAction, Binding)> {
        CUSTOMIZABLE
            .iter()
            .filter_map(|&a| Some((a, self.binding(a)?)))
            .collect()
    }

    pub fn is_custom(&self, action: RowAction) -> bool {
        self.shortcuts.iter().any(|(a, _)| *a == action)
    }

    /// `None` turns the shortcut off.
    pub fn set_binding(
        &mut self,
        action: RowAction,
        binding: Option<Binding>,
    ) -> Result<(), BindError> {
        if let Some(b) = &binding {
            let named =
                b.key.chars().count() == 1 && b.key.chars().all(|c| c.is_ascii_alphanumeric());
            if !named && !PUNCTUATION.contains(&b.key.as_str()) {
                return Err(BindError::Unsupported);
            }
            if RESERVED.contains(&(b.key.as_str(), b.shift)) {
                return Err(BindError::Reserved);
            }
            if let Some((other, _)) = self
                .keymap()
                .into_iter()
                .find(|(a, o)| *a != action && o == b)
            {
                return Err(BindError::InUse(other));
            }
        }
        self.shortcuts.retain(|(a, _)| *a != action);
        if binding != default_binding(action) {
            self.shortcuts.push((action, binding));
        }
        Ok(())
    }

    pub fn reset_binding(&mut self, action: RowAction) -> Result<(), BindError> {
        self.set_binding(action, default_binding(action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_customizable_action_has_a_distinct_default() {
        let defaults: Vec<Binding> = CUSTOMIZABLE
            .iter()
            .map(|&a| default_binding(a).unwrap())
            .collect();
        for (i, b) in defaults.iter().enumerate() {
            assert!(!defaults[i + 1..].contains(b), "{b:?} is used twice");
        }
        assert!(default_binding(RowAction::Delete).is_none());
    }

    // set_binding and binding() both speak Option<Binding>
    #[allow(clippy::unnecessary_wraps)]
    fn b(key: &str, shift: bool) -> Option<Binding> {
        Some(Binding::new(key, shift))
    }

    #[test]
    fn defaults_apply_until_changed() {
        let cfg = Config::default();
        assert_eq!(cfg.binding(RowAction::Pin), b("P", false));
        assert_eq!(cfg.keymap().len(), CUSTOMIZABLE.len());
        assert!(!cfg.is_custom(RowAction::Pin));
    }

    #[test]
    fn set_replaces_and_turns_off() {
        let mut cfg = Config::default();
        cfg.set_binding(RowAction::Pin, b("K", false)).unwrap();
        assert_eq!(cfg.binding(RowAction::Pin), b("K", false));
        assert!(cfg.is_custom(RowAction::Pin));
        // the old combo is free for another action
        cfg.set_binding(RowAction::Comment, b("P", false)).unwrap();
        cfg.set_binding(RowAction::Mute, None).unwrap();
        assert_eq!(cfg.binding(RowAction::Mute), None);
        assert!(!cfg.keymap().iter().any(|(a, _)| *a == RowAction::Mute));
    }

    #[test]
    fn set_rejects_conflicts_reserved_and_unsupported_keys() {
        let mut cfg = Config::default();
        assert_eq!(
            cfg.set_binding(RowAction::Pin, b("D", false)),
            Err(BindError::InUse(RowAction::DiffBase))
        );
        assert_eq!(
            cfg.set_binding(RowAction::Pin, b("F", false)),
            Err(BindError::Reserved)
        );
        assert_eq!(
            cfg.set_binding(RowAction::Pin, b("P", true)),
            Err(BindError::Reserved)
        );
        assert_eq!(
            cfg.set_binding(RowAction::Pin, b("Enter", false)),
            Err(BindError::Unsupported)
        );
        assert_eq!(
            cfg.set_binding(RowAction::Pin, b("Escape", false)),
            Err(BindError::Unsupported)
        );
        // Shift makes a different combo, and rebinding to its own combo is fine
        cfg.set_binding(RowAction::Pin, b("D", true)).unwrap();
        cfg.set_binding(RowAction::Pin, b("D", true)).unwrap();
        assert_eq!(cfg.binding(RowAction::Pin), b("D", true));
    }

    #[test]
    fn a_turned_off_shortcut_frees_its_combo() {
        let mut cfg = Config::default();
        cfg.set_binding(RowAction::DiffBase, None).unwrap();
        cfg.set_binding(RowAction::Pin, b("D", false)).unwrap();
        assert_eq!(cfg.binding(RowAction::Pin), b("D", false));
    }

    #[test]
    fn reset_restores_the_default_unless_another_action_took_it() {
        let mut cfg = Config::default();
        cfg.set_binding(RowAction::Pin, b("K", false)).unwrap();
        cfg.reset_binding(RowAction::Pin).unwrap();
        assert!(!cfg.is_custom(RowAction::Pin));
        assert!(cfg.shortcuts.is_empty());
        cfg.set_binding(RowAction::Pin, b("K", false)).unwrap();
        cfg.set_binding(RowAction::Comment, b("P", false)).unwrap();
        assert_eq!(
            cfg.reset_binding(RowAction::Pin),
            Err(BindError::InUse(RowAction::Comment))
        );
    }

    #[test]
    fn overrides_survive_a_config_roundtrip() {
        let mut cfg = Config::default();
        cfg.set_binding(RowAction::Color(Some(2)), b("K", true))
            .unwrap();
        cfg.set_binding(RowAction::Mute, None).unwrap();
        let back: Config = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back.binding(RowAction::Color(Some(2))), b("K", true));
        assert_eq!(back.binding(RowAction::Mute), None);
    }
}
