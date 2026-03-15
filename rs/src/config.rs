use crate::test_step::TestStepGroup;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

struct ConfigData {
    path: PathBuf,
    parent: Option<Rc<ConfigData>>,
    step_sets: HashMap<String, TestStepGroup>,
}

impl ConfigData {}
