use clap::{ArgAction, Parser};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
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

fn is_config_file(path: &PathBuf) -> bool {
    if !is_yaml(path) {
        return false;
    }
    if let Some(stem) = path.file_stem() {
        return stem == "config" || stem == "yapitest-config";
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

fn try_load_file(path: &PathBuf) -> (Vec<Test>, Option<ConfigData>) {
    if is_test_file(path) {
        // Try Load Test
        if let Some(basename) = path.file_name() {
            let (config, tests) = Test::load_test_file(path);
            //println!("Test: {}", path.display());
            return (tests, config);
        }
    } else if is_config_file(path) {
        // Try Load Config File
        if let Some(basename) = path.file_name() {
            println!("Config: {}", path.display());
            let new_config = ConfigData::from_file(None, path);
            if !new_config.is_some() {
                println!("CONFIG FAILED TO LOAD");
            }
            return (vec![], new_config);
        }
    }

    (vec![], None)
}

fn load_from_path(path: &PathBuf) -> (Vec<Test>, Vec<ConfigData>) {
    let mut test_output: Vec<Test> = vec![];
    let mut config_output: Vec<ConfigData> = vec![];

    if path.is_dir() {
        for entry in WalkDir::new(path).into_iter().filter_map(Result::ok) {
            if entry.path() == path || entry.path().is_dir() {
                continue;
            }

            // println!("Entry: {}", entry.path().display());

            let path_buf = entry.path().to_path_buf();
            let (tests, configs) = load_from_path(&path_buf);
            test_output.extend(tests);
            config_output.extend(configs);
        }
    } else {
        let (tests, configs) = try_load_file(path);
        test_output.extend(tests);
        config_output.extend(configs);
    }

    (test_output, config_output)
    //(vec![], vec![])
}

fn build_config_tree(mut configs: Vec<ConfigData>) -> Vec<Arc<Mutex<ConfigData>>> {
    // Sort by path depth (shorter paths = higher in the tree = potential parents)
    configs.sort_by_key(|c| c.path.components().count());

    let mut arc_configs: Vec<Arc<Mutex<ConfigData>>> = Vec::with_capacity(configs.len());

    // First pass: wrap every ConfigData in Arc<Mutex<...>>
    for config in configs {
        arc_configs.push(Arc::new(Mutex::new(config)));
    }

    // Second pass: link parents
    for i in 0..arc_configs.len() {
        let child_arc = &arc_configs[i];
        let child_path = {
            let guard = child_arc.lock().unwrap();
            guard.path.clone()
        };

        // Look for the best (closest) parent
        for j in 0..i {
            // Only check previous (shallower) configs
            let potential_parent_arc = &arc_configs[j];
            let parent_dir = {
                let guard = potential_parent_arc.lock().unwrap();
                guard.path.parent().map(|p| p.to_path_buf())
            };

            if let Some(parent_dir) = parent_dir {
                if child_path.starts_with(&parent_dir) {
                    // Found a parent! Set it on the child
                    let mut child_guard = child_arc.lock().unwrap();
                    child_guard.set_parent(Some(Arc::clone(potential_parent_arc)));
                    break; // Stop at the first (closest) parent
                }
            }
        }
    }

    arc_configs
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

fn main() {
    let args = Args::parse();

    // Validate Paths Exist

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
                    eprintln!("{}", e);
                    panic!("Error Unwrapping Path {}", path_arg);
                }
            }
        } else {
            panic!("Path \"{}\" does not exist. Exiting.", path_arg)
        }
    }

    // Gather All Tests & Configs
    println!("Gathering Tests & Configs");
    let mut tests: Vec<Test> = vec![];
    let mut configs: Vec<ConfigData> = vec![];
    for path in test_paths.iter() {
        let (path_tests, path_configs) = load_from_path(path);
        tests.extend(path_tests);
        configs.extend(path_configs);
    }

    let configs = build_config_tree(configs);

    for config in configs.iter() {
        match config.lock() {
            Ok(cfg) => {
                println!("CONFIG: {}", cfg.path.display());
                if cfg.parent.is_some() {
                    println!("HAS PARENT: {}", cfg.path.display());
                } else {
                    println!("NO PARENT: {}", cfg.path.display());
                }
            }
            Err(_e) => {}
        }
    }

    return;

    println!("Groups");
    for path in args.group.iter() {
        println!("{}", path);
    }

    println!("Exclude");
    for path in args.exclude.iter() {
        println!("{}", path);
    }

    println!("Include");
    for path in args.include.iter() {
        println!("{}", path);
    }
}
