use super::config::Config;
use super::rule::{Rule, RuleCollection};
use std::collections::HashMap;

impl Config {
    // Ids only go up, so a deleted collection's id never lands on a later one (rules or an
    // exported file may still mention it).
    fn new_collection_id(&mut self) -> u64 {
        let used = self.collections.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let id = self.next_collection_id.max(used);
        self.next_collection_id = id + 1;
        id
    }

    pub fn add_collection(&mut self, name: &str) -> u64 {
        let id = self.new_collection_id();
        self.collections.push(RuleCollection {
            id,
            name: name.to_string(),
            enabled: true,
        });
        id
    }

    pub fn collection_exists(&self, id: u64) -> bool {
        self.collections.iter().any(|c| c.id == id)
    }

    pub fn set_collection_enabled(&mut self, id: u64, on: bool) -> bool {
        if let Some(c) = self.collections.iter_mut().find(|c| c.id == id) {
            c.enabled = on;
            true
        } else {
            false
        }
    }

    /// Turns this collection on and every other one off. Ungrouped rules are always active, so they stay as they are.
    pub fn solo_collection(&mut self, id: u64) -> bool {
        if !self.collection_exists(id) {
            return false;
        }
        for c in &mut self.collections {
            c.enabled = c.id == id;
        }
        true
    }

    /// `false` when the rule or the collection doesn't exist.
    pub fn assign_rule(&mut self, rule: u64, collection: Option<u64>) -> bool {
        if collection.is_some_and(|c| !self.collection_exists(c)) {
            return false;
        }
        if let Some(r) = self.rules.iter_mut().find(|r| r.id == rule) {
            r.collection = collection;
            true
        } else {
            false
        }
    }

    /// Per collection (`None` is Ungrouped): rules in it, and how many of those have their own switch on.
    pub fn collection_counts(&self) -> HashMap<Option<u64>, (usize, usize)> {
        let mut counts = HashMap::new();
        for r in &self.rules {
            let (total, on) = counts.entry(r.collection).or_insert((0, 0));
            *total += 1;
            *on += usize::from(r.enabled);
        }
        counts
    }

    pub fn disabled_collections(&self) -> Vec<u64> {
        self.collections
            .iter()
            .filter(|c| !c.enabled)
            .map(|c| c.id)
            .collect()
    }

    /// Rules move to Ungrouped unless `delete_rules` is set.
    pub fn delete_collection(&mut self, id: u64, delete_rules: bool) {
        self.collections.retain(|c| c.id != id);
        if delete_rules {
            self.rules.retain(|r| r.collection != Some(id));
        } else {
            for r in self.rules.iter_mut().filter(|r| r.collection == Some(id)) {
                r.collection = None;
            }
        }
    }

    /// Appends imported rules and collections under fresh ids, so nothing clashes with what is here.
    /// A rule pointing at a collection the file doesn't define lands in Ungrouped.
    pub fn merge_rules(&mut self, collections: Vec<RuleCollection>, rules: Vec<Rule>) {
        let mut remap = HashMap::new();
        for c in collections {
            let id = self.new_collection_id();
            let name = c.name.trim();
            self.collections.push(RuleCollection {
                id,
                name: if name.is_empty() {
                    "Untitled collection".into()
                } else {
                    name.into()
                },
                enabled: c.enabled,
            });
            remap.insert(c.id, id);
        }
        let first = self.next_rule_id();
        for (id, r) in (first..).zip(rules) {
            let collection = r.collection.and_then(|c| remap.get(&c).copied());
            self.rules.push(Rule {
                id,
                collection,
                ..r
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::rule::Action;

    fn rule(id: u64, pattern: &str, collection: Option<u64>) -> Rule {
        Rule {
            id,
            enabled: true,
            name: String::new(),
            pattern: pattern.into(),
            method: String::new(),
            action: Action::Block { status: 503 },
            collection,
            hits: 0,
        }
    }

    fn hits(c: &mut Config, url: &str) -> Vec<u64> {
        c.matching("GET", url, |r| r.id)
    }

    #[test]
    fn rule_is_active_only_when_it_and_its_collection_are_on() {
        let mut c = Config::default();
        let a = c.add_collection("A");
        c.rules = vec![rule(1, "x.com", None), rule(2, "x.com", Some(a))];
        assert_eq!(hits(&mut c, "https://x.com/"), vec![1, 2]);
        c.set_collection_enabled(a, false);
        assert_eq!(hits(&mut c, "https://x.com/"), vec![1]);
        c.set_collection_enabled(a, true);
        c.rules[1].enabled = false;
        assert_eq!(hits(&mut c, "https://x.com/"), vec![1]);
        c.rules[1].collection = Some(99);
        c.rules[1].enabled = true;
        assert_eq!(
            hits(&mut c, "https://x.com/"),
            vec![1, 2],
            "a dangling collection id doesn't hide the rule"
        );
    }

    #[test]
    fn turning_a_collection_off_keeps_each_rules_own_switch() {
        let mut c = Config::default();
        let a = c.add_collection("A");
        c.rules = vec![rule(1, "x.com", Some(a)), rule(2, "y.com", Some(a))];
        c.rules[1].enabled = false;
        c.set_collection_enabled(a, false);
        c.set_collection_enabled(a, true);
        assert_eq!(
            c.rules.iter().map(|r| r.enabled).collect::<Vec<_>>(),
            vec![true, false]
        );
    }

    #[test]
    fn solo_enables_one_collection_and_leaves_ungrouped_alone() {
        let mut c = Config::default();
        let (a, b) = (c.add_collection("A"), c.add_collection("B"));
        c.rules = vec![
            rule(1, "x.com", None),
            rule(2, "x.com", Some(a)),
            rule(3, "x.com", Some(b)),
        ];
        assert!(c.solo_collection(b));
        assert_eq!(hits(&mut c, "https://x.com/"), vec![1, 3]);
        assert!(!c.solo_collection(77));
        assert_eq!(hits(&mut c, "https://x.com/"), vec![1, 3]);
    }

    #[test]
    fn deleting_a_collection_moves_or_removes_its_rules() {
        let build = || {
            let mut c = Config::default();
            let a = c.add_collection("A");
            c.rules = vec![rule(1, "x.com", Some(a)), rule(2, "y.com", None)];
            (c, a)
        };
        let (mut c, a) = build();
        c.delete_collection(a, false);
        assert!(c.collections.is_empty());
        assert_eq!(c.rules.len(), 2);
        assert!(c.rules.iter().all(|r| r.collection.is_none()));

        let (mut c, a) = build();
        c.delete_collection(a, true);
        assert_eq!(c.rules.iter().map(|r| r.id).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn merge_remaps_rule_and_collection_ids() {
        let mut c = Config::default();
        let mine = c.add_collection("Mine");
        c.rules = vec![rule(1, "mine.com", Some(mine))];
        let incoming = vec![RuleCollection {
            id: 1,
            name: "Theirs".into(),
            enabled: false,
        }];
        c.merge_rules(
            incoming,
            vec![rule(1, "a.com", Some(1)), rule(2, "b.com", Some(42))],
        );
        assert_eq!(c.collections.len(), 2);
        let theirs = c.collections.iter().find(|x| x.name == "Theirs").unwrap();
        assert_ne!(theirs.id, mine);
        assert!(!theirs.enabled);
        assert_eq!(c.rules[0].collection, Some(mine));
        assert_eq!(c.rules[1].collection, Some(theirs.id));
        assert_eq!(c.rules[2].collection, None, "unknown collection id");
        let ids: Vec<u64> = c.rules.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn collection_ids_are_not_reused_after_a_delete() {
        let mut c = Config::default();
        let a = c.add_collection("A");
        c.delete_collection(a, false);
        assert_ne!(c.add_collection("B"), a);
        // a config saved before the counter existed still avoids its highest id
        let mut old = Config::default();
        old.collections.push(RuleCollection {
            id: 5,
            name: "X".into(),
            enabled: true,
        });
        assert_eq!(old.add_collection("Y"), 6);
    }

    #[test]
    fn counts_and_assignment_follow_the_rules() {
        let mut c = Config::default();
        let a = c.add_collection("A");
        c.rules = vec![
            rule(1, "x", Some(a)),
            rule(2, "y", Some(a)),
            rule(3, "z", None),
        ];
        c.rules[1].enabled = false;
        let counts = c.collection_counts();
        assert_eq!(counts[&Some(a)], (2, 1));
        assert_eq!(counts[&None], (1, 1));
        assert!(c.assign_rule(3, Some(a)));
        assert!(!c.assign_rule(3, Some(99)) && !c.assign_rule(42, None));
        assert_eq!(c.rules[2].collection, Some(a));
    }

    #[test]
    fn merge_trims_names_and_names_blank_ones() {
        let mut c = Config::default();
        let blank = |id, name: &str| RuleCollection {
            id,
            name: name.into(),
            enabled: true,
        };
        c.merge_rules(vec![blank(1, "  "), blank(2, " Slow ")], vec![]);
        let names: Vec<&str> = c.collections.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["Untitled collection", "Slow"]);
    }

    #[test]
    fn config_without_collections_loads_with_rules_ungrouped() {
        let c: Config = serde_json::from_str(
            r#"{"rules":[{"id":1,"enabled":true,"pattern":"a.com","method":"","action":{"Block":{"status":503}}}]}"#,
        )
        .unwrap();
        assert!(c.collections.is_empty());
        assert_eq!(c.rules[0].collection, None);
    }
}
