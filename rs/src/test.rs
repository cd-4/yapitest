use serde::Deserialize;
use serde_yaml::{Value, from_value};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use crate::config::ConfigSpec;
use crate::test_spec::TestSpec;

pub struct Test {
    name: String,
}

fn is_test_name(key: String) -> bool {
    let lower_name = key.to_lowercase();
    lower_name.starts_with("test") || lower_name.ends_with("test")
}

impl Test {
    pub fn load_test_file(path: &PathBuf) {
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            let test_file_result = serde_yaml::from_reader::<_, Value>(reader);
            match test_file_result {
                Ok(tests_file) => {
                    println!("Loaded Test File");

                    if let Some(mapping) = tests_file.as_mapping() {
                        for key in mapping.keys().filter_map(|v| v.as_str()) {
                            if key == "config" {
                                if let Some(config_value) = mapping.get(key) {
                                    match from_value::<ConfigSpec>(config_value.clone()) {
                                        Ok(config_spec) => {
                                            println!("Loaded Config Data: {:?}", config_spec);
                                        }
                                        Err(e) => {
                                            panic!(
                                                "Failed to parse test config: {}\n{}",
                                                path.display(),
                                                e
                                            );
                                        }
                                    }
                                }
                                continue;
                            } else if is_test_name(key.to_string()) {
                                if let Some(config_value) = mapping.get(key) {
                                    match from_value::<TestSpec>(config_value.clone()) {
                                        Ok(test_spec) => {
                                            println!("Loaded Test Data: {:?}", test_spec);
                                        }
                                        Err(e) => {
                                            panic!(
                                                "Failed to parse test: {} at {}\n{}",
                                                key,
                                                path.display(),
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            eprintln!("Key: {}", key);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error Loading Test File: {}\n{}", path.display(), e);
                }
            }
        }
    }
}
