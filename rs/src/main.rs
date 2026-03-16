use clap::{ArgAction, Parser};
use std::env;
use std::path::{Path, PathBuf};

mod config;
mod test_step;

use crate::config::ConfigData;

fn collect_configs(paths: Vec<PathBuf>) -> Vec<ConfigData> {
    vec![]
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

    let configs = collect_configs(test_paths);

    /*
        println!("Paths");
        for path in test_paths.iter() {
            println!("{}", path.display());
        }
    */

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
