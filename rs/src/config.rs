use serde_json::Value;

use crate::test_step::TestStepGroup;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

struct ConfigData {
    path: PathBuf,
    parent: Option<Rc<ConfigData>>,
    step_sets: HashMap<String, TestStepGroup>,
    data: Value,
}

impl ConfigData {
    pub fn get_step_set(&self, key: String) -> Option<&TestStepGroup> {
        let retrieved_value = self.step_sets.get(&key);
        match retrieved_value {
            Some(value) => {
                return Some(value);
            }
            None => match &self.parent {
                Some(parent) => parent.get_step_set(key),
                None => None,
            },
        }
    }

    pub fn get_keys(&self, Vec<String>) Value {
    }
}
