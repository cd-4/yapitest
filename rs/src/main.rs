use clap::{ArgAction, Parser};
use colored::Colorize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use yapitest::{load_tests, print_test_results, run_tests, Test, TestResult};

// ── CTRF report structs ──────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CtrfReport {
    report_format: &'static str,
    spec_version: &'static str,
    results: CtrfResults,
}

#[derive(Serialize)]
struct CtrfResults {
    tool: CtrfTool,
    summary: CtrfSummary,
    tests: Vec<CtrfTest>,
}

#[derive(Serialize)]
struct CtrfTool {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct CtrfSummary {
    tests: usize,
    passed: usize,
    failed: usize,
    skipped: usize,
    pending: usize,
    other: usize,
    start: u64,
    stop: u64,
}

#[derive(Serialize)]
struct CtrfTest {
    name: String,
    status: &'static str,
    duration: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(rename = "filePath", skip_serializing_if = "Option::is_none")]
    file_path: Option<String>,
}

fn write_ctrf_report(
    output_path: &str,
    test_results: &[TestResult],
    start_ms: u64,
    stop_ms: u64,
) -> anyhow::Result<()> {
    let passed = test_results.iter().filter(|r| r.passed()).count();
    let failed = test_results.len() - passed;

    let tests: Vec<CtrfTest> = test_results
        .iter()
        .map(|r| CtrfTest {
            name: r.name().to_owned(),
            status: if r.passed() { "passed" } else { "failed" },
            duration: r.duration_ms,
            message: r.get_failure_message().map(|s| s.to_owned()),
            file_path: r.file_path().map(|p| p.display().to_string()),
        })
        .collect();

    let report = CtrfReport {
        report_format: "CTRF",
        spec_version: "1.0.0",
        results: CtrfResults {
            tool: CtrfTool {
                name: "yapitest",
                version: env!("CARGO_PKG_VERSION"),
            },
            summary: CtrfSummary {
                tests: test_results.len(),
                passed,
                failed,
                skipped: 0,
                pending: 0,
                other: 0,
                start: start_ms,
                stop: stop_ms,
            },
            tests,
        },
    };

    let file = std::fs::File::create(output_path)
        .map_err(|e| anyhow::anyhow!("Failed to create output file '{}': {}", output_path, e))?;
    serde_json::to_writer_pretty(file, &report)
        .map_err(|e| anyhow::anyhow!("Failed to write CTRF report: {}", e))?;
    Ok(())
}

#[derive(Parser, Debug)]
#[command(version, about = "Yapitest is a simple API testing platform")]
struct Args {
    paths: Vec<String>,

    // Test Groups to run
    #[arg(short = 'g', action = ArgAction::Append)]
    group: Vec<String>,

    // Tests to exclude
    #[arg(short = 'x', action = ArgAction::Append)]
    exclude: Vec<String>,

    // Tests to include
    #[arg(short = 'i', action = ArgAction::Append)]
    include: Vec<String>,

    // Exact test name match
    #[arg(short = 'k', action = ArgAction::Append)]
    key: Vec<String>,

    // Number of threads
    #[arg(short = 't')]
    threads: Option<u64>,

    // Output verbosity: 0=silent, 1=names only, 2=default, 3=assertions
    #[arg(short = 'v', default_value_t = 2)]
    verbosity: u8,

    // Write a CTRF JSON report to this file
    #[arg(long = "output")]
    output: Option<String>,
}

#[tokio::main]
async fn main() {
    let start_time = SystemTime::now();

    let args = Args::parse();

    let path_args: Vec<String> = if args.paths.is_empty() {
        vec![".".to_string()]
    } else {
        args.paths.clone()
    };

    let mut test_paths: Vec<PathBuf> = Vec::new();
    for path_arg in &path_args {
        let path = PathBuf::from(path_arg);
        if path.exists() {
            match std::fs::canonicalize(&path) {
                Ok(p) => test_paths.push(p),
                Err(e) => panic!("Error Unwrapping Path {}: {}", path_arg, e),
            }
        } else {
            panic!("Path \"{}\" does not exist. Exiting.", path_arg)
        }
    }

    let verbosity = args.verbosity;

    if verbosity >= 2 {
        let divider = "─".repeat(40);
        println!("yapitest v{}", env!("CARGO_PKG_VERSION"));
        println!("{}", divider.dimmed());
    }

    let mut configs: HashMap<PathBuf, std::sync::Arc<std::sync::RwLock<yapitest::ConfigData>>> =
        HashMap::new();
    let mut tests: Vec<Test> = vec![];
    if verbosity >= 2 {
        println!("{}", "Collecting tests...".dimmed());
    }
    for path in &test_paths {
        match load_tests(&mut configs, path) {
            Ok(found_tests) => tests.extend(found_tests),
            Err(e) => panic!("{}", e),
        }
    }

    fn contains_group(test: &Test, groups: &[&String]) -> bool {
        test.groups
            .as_ref()
            .is_some_and(|tg| groups.iter().any(|g| tg.contains(*g)))
    }

    fn contains_text(test: &Test, texts: &[&String]) -> bool {
        texts.iter().any(|t| test.name.contains(t.as_str()))
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

    if !args.key.is_empty() {
        tests.retain(|t| args.key.iter().any(|k| t.name == *k));
    }

    if verbosity >= 2 {
        println!("{}", format!("Found {} tests", tests.len()).dimmed());
        println!();
    }
    let start_ms = start_time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let test_results = run_tests(&tests, args.threads, verbosity).await;

    let end_time = SystemTime::now();
    let stop_ms = end_time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let duration = end_time
        .duration_since(start_time)
        .expect("Time went backwards")
        .as_secs_f32();
    print_test_results(&test_results, duration, verbosity);

    if let Some(output_path) = &args.output {
        if let Err(e) = write_ctrf_report(output_path, &test_results, start_ms, stop_ms) {
            eprintln!("Error writing CTRF report: {}", e);
        }
    }

    let any_failed = test_results.iter().any(|r| !r.passed());
    std::process::exit(if any_failed { 1 } else { 0 });
}
