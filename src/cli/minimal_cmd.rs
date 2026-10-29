use super::args::parse_value;
use super::minimal::{collect_minimal, parse_board};
use std::path::PathBuf;
use sudoku_solver::archive::analyze_minimality_from_puzzle;

pub(crate) fn run(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let command = args.next().ok_or_else(|| usage().to_owned())?;
    let mut archive_path = PathBuf::from("data/minimal-puzzle-archive.json");
    let mut checkpoint_path = None;
    let mut nodes = 100_000u64;
    let mut checks = 10_000u64;
    let mut clues = None;
    let mut target = None;
    let mut attempts = 1_000u64;
    let mut remove_count = None;
    let mut add_count = None;
    let mut seed = 20_261_027u64;
    let mut to_clues = None;
    let mut puzzle = None;

    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value after {option}\n{}", usage()))?;
        match option.as_str() {
            "--archive" => archive_path = PathBuf::from(value),
            "--checkpoint" => checkpoint_path = Some(PathBuf::from(value)),
            "--nodes" => nodes = parse_value(&value, "--nodes")?,
            "--checks" => checks = parse_value(&value, "--checks")?,
            "--clues" => clues = Some(parse_value(&value, "--clues")?),
            "--target" => target = Some(parse_value(&value, "--target")?),
            "--attempts" => attempts = parse_value(&value, "--attempts")?,
            "--remove" => remove_count = Some(parse_value(&value, "--remove")?),
            "--add" => add_count = Some(parse_value(&value, "--add")?),
            "--seed" => seed = parse_value(&value, "--seed")?,
            "--to" => to_clues = Some(parse_value(&value, "--to")?),
            "--puzzle" => puzzle = Some(value),
            _ => return Err(format!("unknown option {option}\n{}", usage())),
        }
    }
    if nodes == 0 || checks == 0 {
        return Err("--nodes and --checks must be non-zero".into());
    }

    match command.as_str() {
        "verify" => {
            let encoded = puzzle.ok_or_else(|| usage().to_owned())?;
            let board = parse_board(&encoded)?;
            let analysis =
                analyze_minimality_from_puzzle(board, nodes).map_err(|error| error.to_string())?;
            println!(
                "clues={} status={:?} unique={} puzzle_key={} solution_class={} nodes={}",
                analysis.clue_count,
                analysis.status,
                analysis.evidence.input_unique,
                analysis.puzzle_key,
                analysis.solution_class,
                analysis.evidence.total_nodes
            );
        }
        "collect" => {
            let source_clues = clues.ok_or_else(|| usage().to_owned())?;
            let target_count = target.ok_or_else(|| usage().to_owned())?;
            if source_clues == 0 || source_clues > 81 || attempts == 0 {
                return Err("--clues must be 1..=81 and --attempts must be non-zero".into());
            }
            let (remove_count, add_count) = match (remove_count, add_count) {
                (Some(remove), Some(add)) => (remove, add),
                (None, None) => (1, 1),
                _ => return Err("--remove and --add must be specified together".into()),
            };
            if remove_count == 0 || remove_count > source_clues {
                return Err("--remove must be in 1..=--clues".into());
            }
            let available_after_removal = 81 - source_clues + remove_count;
            if add_count > available_after_removal {
                return Err("--add exceeds the number of empty cells after removal".into());
            }
            let inferred_target = source_clues - remove_count + add_count;
            let target_clues = to_clues.unwrap_or(inferred_target);
            if target_clues == 0 || target_clues != inferred_target {
                return Err(format!(
                    "--to must equal --clues - --remove + --add ({inferred_target})"
                ));
            }
            let checkpoint = checkpoint_path.unwrap_or_else(|| {
                if source_clues == target_clues && remove_count == 1 && add_count == 1 {
                    PathBuf::from(format!("data/minimal-collect-{source_clues}.json"))
                } else {
                    PathBuf::from(format!(
                        "data/minimal-collect-{source_clues}-to-{target_clues}-{remove_count}-{add_count}.json"
                    ))
                }
            });
            collect_minimal(
                archive_path,
                checkpoint,
                source_clues,
                target_clues,
                remove_count,
                add_count,
                target_count,
                attempts,
                seed,
                nodes,
                checks,
            )?;
        }
        _ => return Err(format!("unknown minimal command {command:?}\n{}", usage())),
    }
    Ok(())
}

pub(crate) fn usage() -> &'static str {
    "usage: sudoku minimal <verify|collect> [--puzzle 81_DIGITS] [--clues SOURCE_N] [--to TARGET_N] [--remove R --add S] [--target COUNT] [--attempts N] [--seed N] [--checkpoint PATH] [--archive PATH] [--nodes N] [--checks N]"
}
