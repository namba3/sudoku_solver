use super::minimal::{collect_minimal, parse_board};
use std::path::PathBuf;
use sudoku_solver::archive::analyze_minimality_from_puzzle;

pub(crate) fn run(command: super::MinimalCommand) -> Result<(), String> {
    let archive_path = PathBuf::from("data/minimal-puzzle-archive.json");
    match command {
        super::MinimalCommand::Verify(args) => {
            let nodes = args.nodes.unwrap_or(100_000);
            let checks = args.checks.unwrap_or(10_000);
            if nodes == 0 || checks == 0 {
                return Err("--nodes and --checks must be non-zero".into());
            }
            let puzzle = args.puzzle.ok_or_else(|| "missing --puzzle".to_owned())?;
            let board = parse_board(&puzzle)?;
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
        super::MinimalCommand::Collect(args) => {
            let source_clues = args.clues.ok_or_else(|| "missing --clues".to_owned())?;
            let target_count = args.target.ok_or_else(|| "missing --target".to_owned())?;
            let nodes = args.nodes.unwrap_or(100_000);
            let checks = args.checks.unwrap_or(10_000);
            let attempts = args.attempts.unwrap_or(1_000);
            let seed = args.seed.unwrap_or(20_261_027);
            let remove_count = args.remove;
            let add_count = args.add;
            let to_clues = args.to;
            let checkpoint_path = args.checkpoint;
            let archive_path = args.archive.unwrap_or(archive_path);
            if nodes == 0 || checks == 0 {
                return Err("--nodes and --checks must be non-zero".into());
            }

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
    }
    Ok(())
}
