use serde::Deserialize;
use serde_yaml::Value;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use crate::test_spec::TestSpec;

pub struct Test {
    name: String,
}

impl Test {
    pub fn load_test_file(path: &PathBuf) {
        if let Ok(file) = File::open(path) {
            // 2. Wrap it in a BufReader for efficiency
            let reader = BufReader::new(file);
            // 3. Deserialize directly from the reader
            let test_file_result = serde_yaml::from_reader::<_, Value>(reader);
            match test_file_result {
                Ok(tests_file) => {
                    println!("Loaded Test File");
                }
                Err(e) => {
                    eprintln!("{}", e);
                }
            }
        }
    }
}
