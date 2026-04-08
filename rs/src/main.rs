use anyhow::{Error, Result, anyhow};
use clap::{ArgAction, Parser};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};
use walkdir::WalkDir;

mod config;
mod test;
mod test_step;

use crate::config::ConfigData;
use crate::test::Test;

fn is_yaml(path: &PathBuf) -> bool {
    if let Some(extension) = path.extension() {
        return extension == "yaml" || extension == "yml";
    }
    false
}

fn is_test_file(path: &PathBuf) -> bool {
    if !is_yaml(path) {
        return false;
    }
    if let Some(stem) = path
        .file_stem()
        .and_then(|v| v.to_str())
        .map(|v| v.to_lowercase())
    {
        return stem.starts_with("test") || stem.ends_with("test");
    }
    false
}

fn is_root_dir(path: &PathBuf) -> bool {
    if !path.is_dir() {
        return false;
    }

    let mut path_copy = path.clone();
    path_copy.push(".git");

    if path_copy.exists() {
        return true;
    }

    false
}

#[derive(Parser, Debug)]
#[command(version, about = "Simple example with positional args")]
struct Args {
    paths: Vec<String>,

    #[arg(short = 'g', action = ArgAction::Append)]
    group: Vec<String>,

    #[arg(short = 'x', action = ArgAction::Append)]
    exclude: Vec<String>,

    #[arg(short = 'i', action = ArgAction::Append)]
    include: Vec<String>,
}

fn get_config_in_dir(path: &PathBuf) -> Result<Option<ConfigData>> {
    let yapitest_config_names = [
        "yapitest-config.yaml",
        "yapitest-config.yml",
        "config.yaml",
        "config.yml",
    ];
    for config_name in yapitest_config_names.iter() {
        let mut config_path = path.clone();
        config_path.push(config_name);
        if config_path.exists() {
            match ConfigData::from_file(&config_path) {
                Ok(config) => {
                    return Ok(Some(config));
                }
                Err(e) => {
                    return Err(anyhow!("{}", e));
                }
            }
        }
    }
    Ok(None)
}

fn load_tests_from_file(
    configs: &mut HashMap<PathBuf, Arc<RwLock<ConfigData>>>,
    path: &PathBuf,
) -> anyhow::Result<Vec<Test>, anyhow::Error> {
    if !is_test_file(path) {
        return Ok(vec![]);
    }

    let mut deepest_config_key: Option<PathBuf> = None;

    let (cfg_opt, mut tests) = Test::load_from_file(path)?;

    // If a config exists, set the test's config to it, and declare it as deepest config
    if let Some(config) = cfg_opt.and_then(|v| Some(Arc::new(RwLock::new(v)))) {
        deepest_config_key = Some(config.read().unwrap().path.clone());
        configs.insert(config.read().unwrap().path.clone(), Arc::clone(&config));
        for test in tests.iter_mut() {
            test.add_config(Arc::clone(&config));
        }
    }

    for ancestor in path.ancestors() {
        let ancestor_pb = ancestor.to_path_buf();

        let mut ancestor_config: Option<Arc<RwLock<ConfigData>>> = None;

        // Get Ancestor Config if it exists
        if let Some(anc_config) = configs.get(ancestor) {
            ancestor_config = Some(Arc::clone(&anc_config));
        } else {
            match get_config_in_dir(&ancestor_pb) {
                Ok(anc_config_opt) => {
                    if let Some(anc_config) = anc_config_opt {
                        let arc_anc_config = Arc::new(RwLock::new(anc_config));
                        configs.insert(ancestor_pb.clone(), Arc::clone(&arc_anc_config));
                        ancestor_config = Some(Arc::clone(&arc_anc_config));
                    }
                }
                Err(e) => {
                    return Err(anyhow!(e));
                }
            }
        }

        // Set Ancestor config as parent of tests & configs
        if let Some(anc_config) = ancestor_config {
            if let Some(deepest_config) = deepest_config_key
                .and_then(|k| configs.get_mut(&k))
                .and_then(|a| Arc::get_mut(a))
            {
                deepest_config
                    .write()
                    .unwrap()
                    .set_parent(Arc::clone(&anc_config));
            }

            for test in tests.iter_mut() {
                test.add_config(Arc::clone(&anc_config));
            }
            deepest_config_key = Some(ancestor_pb);
        }

        // Found root of file system or `.git` file. Exit
        if is_root_dir(&ancestor.to_path_buf()) {
            break;
        }
    }

    Ok(tests)
}

fn load_tests_in_dir(
    configs: &mut HashMap<PathBuf, Arc<RwLock<ConfigData>>>,
    path: &PathBuf,
) -> anyhow::Result<Vec<Test>, anyhow::Error> {
    let mut output: Vec<Test> = vec![];

    if let Ok(read_dir) = std::fs::read_dir(path) {
        for item_res in read_dir {
            match item_res {
                Ok(item) => {
                    if item.path().is_dir() {
                        match load_tests_in_dir(configs, &item.path()) {
                            Ok(new_tests) => {
                                output.extend(new_tests);
                            }
                            Err(e) => {
                                panic!("{}", e);
                            }
                        }
                    } else {
                        match load_tests_from_file(configs, &item.path()) {
                            Ok(new_tests) => {
                                output.extend(new_tests);
                            }
                            Err(e) => {
                                panic!("{}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    panic!("{}", e);
                }
            }
        }
    }

    Ok(output)
}

fn load_tests(
    configs: &mut HashMap<PathBuf, Arc<RwLock<ConfigData>>>,
    path: &PathBuf,
) -> anyhow::Result<Vec<Test>, anyhow::Error> {
    if path.is_dir() {
        load_tests_in_dir(configs, path)
    } else {
        load_tests_from_file(configs, path)
    }
}

async fn run_tests(tests: &mut Vec<Test>) {
    for test in tests.iter() {
        println!("-----------------------------");
        let _x = test.run().await;
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let mut test_paths: Vec<PathBuf> = Vec::new();
    for path_arg in args.paths.iter() {
        let path = PathBuf::from(path_arg);
        if path.exists() {
            let absolute_path = std::fs::canonicalize(&path);
            match absolute_path {
                Ok(p) => {
                    test_paths.push(p);
                }
                Err(e) => {
                    panic!("Error Unwrapping Path {}: {}", path_arg, e);
                }
            }
        } else {
            panic!("Path \"{}\" does not exist. Exiting.", path_arg)
        }
    }

    let mut configs: HashMap<PathBuf, Arc<RwLock<ConfigData>>> = HashMap::new();
    let mut tests: Vec<Test> = vec![];
    println!("Collecting Tests...");
    for path in test_paths.iter() {
        match load_tests(&mut configs, path) {
            Ok(found_tests) => {
                tests.extend(found_tests);
            }
            Err(e) => {
                panic!("{}", e);
            }
        }
    }

    fn contains_group(test: &Test, groups: &Vec<&String>) -> bool {
        if let Some(test_groups) = &test.groups {
            for group in groups.iter() {
                if test_groups.contains(group) {
                    return true;
                }
            }
        }
        false
    }

    fn contains_text(test: &Test, texts: &Vec<&String>) -> bool {
        for text in texts.iter() {
            if test.name.contains(*text) {
                return true;
            }
        }
        false
    }

    if !args.group.is_empty() {
        let groups: Vec<&String> = args.group.iter().collect();
        tests.retain(|t| contains_group(t, &groups));
    }

    if !args.include.is_empty() {
        let includes: Vec<&String> = args.include.iter().collect();
        tests.retain(|t| contains_text(t, &includes));
    }

    if !args.exclude.is_empty() {
        let excludes: Vec<&String> = args.exclude.iter().collect();
        tests.retain(|t| !contains_text(t, &excludes));
    }

    println!("Collected {} Tests", tests.len());
    run_tests(&mut tests).await;
}
