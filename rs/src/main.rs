use clap::{ArgAction, Parser};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

mod config;
mod test;
mod test_spec;
mod test_step;

use crate::config::ConfigData;
use crate::test::Test;

fn try_load_file(path: &PathBuf) -> (Vec<Test>, Vec<ConfigData>) {
    if let Some(basename) = path.file_name() {
        println!("{}", basename.display());
    }

    (vec![], vec![])
}

fn load_from_path(path: &PathBuf) -> (Vec<Test>, Vec<ConfigData>) {
    let mut test_output: Vec<Test> = vec![];
    let mut config_output: Vec<ConfigData> = vec![];

    if path.is_dir() {
        for entry in WalkDir::new(path).into_iter().filter_map(Result::ok) {
            if entry.path() == path {
                continue;
            }

            println!("Entry: {}", entry.path().display());

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

    //collect_test_files(test_paths);
    //let configs = collect_configs(test_paths);

    println!("Paths");
    for path in test_paths.iter() {
        let (tests, configs) = load_from_path(path);
        // println!("{}", path.display());
    }

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
