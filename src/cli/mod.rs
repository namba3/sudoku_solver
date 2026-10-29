mod archive_cmd;
mod args;
mod atomic_file;
mod frontier;
mod minimal;
mod minimal_cmd;
mod minimal_scan;
mod pool;
mod pool_args;
mod symmetry_bench;

pub(crate) fn run(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let command = args.next().ok_or_else(|| usage().to_owned())?;
    let arguments: Vec<_> = args.collect();
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        println!("{}", command_usage(&command));
        return Ok(());
    }
    let mut args = arguments.into_iter();
    match command.as_str() {
        "archive" => archive_cmd::run(args),
        "minimal" => minimal_cmd::run(args),
        "frontier" => frontier::run_cli(args),
        "diverse" => pool::run(args),
        "scan" => minimal_scan::run(args),
        "bench" => match args.next().as_deref() {
            Some("symmetry") => symmetry_bench::run(args),
            _ => Err("expected `bench symmetry`\n".to_owned() + usage()),
        },
        "--help" | "-h" | "help" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(format!("unknown command {command:?}\n{}", usage())),
    }
}

fn command_usage(command: &str) -> String {
    match command {
        "archive" => archive_cmd::usage().into(),
        "minimal" => minimal_cmd::usage().into(),
        "frontier" => frontier::usage().into(),
        "diverse" => pool_args::usage(),
        "scan" => minimal_scan::usage(),
        "bench" => symmetry_bench::usage().into(),
        _ => usage().into(),
    }
}

fn usage() -> &'static str {
    "usage: sudoku <COMMAND> [OPTIONS]\n\
     commands:\n\
     \x20 archive <stats|verify|import|export|seed-fast-bases>\n\
     \x20 minimal <verify|collect>\n\
     \x20 frontier descend\n\
     \x20 diverse [generation options]\n\
     \x20 scan [parallel minimal-search options]\n\
     \x20 bench symmetry [--iterations N]\n\
     use `sudoku <COMMAND> --help` for command-specific usage."
}
