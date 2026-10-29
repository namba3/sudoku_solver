use super::args::parse_value;
use std::path::{Path, PathBuf};
use sudoku_solver::archive::{analyze_minimality, MinimalPuzzleArchive, Provenance};
use sudoku_solver::generator::verified_17_clue_bases;

pub(crate) fn run(mut args: impl Iterator<Item = String>) -> Result<(), String> {
    let command = args.next().ok_or_else(|| usage().to_owned())?;
    let mut archive_path = PathBuf::from("data/minimal-puzzle-archive.json");
    let mut source_path = None;
    let mut output_path = None;
    let mut nodes = 100_000u64;

    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value after {option}\n{}", usage()))?;
        match option.as_str() {
            "--archive" => archive_path = PathBuf::from(value),
            "--source" => source_path = Some(PathBuf::from(value)),
            "--output" => output_path = Some(PathBuf::from(value)),
            "--nodes" => nodes = parse_value(&value, "--nodes")?,
            _ => return Err(format!("unknown option {option}\n{}", usage())),
        }
    }
    if nodes == 0 {
        return Err("--nodes must be non-zero".into());
    }

    match command.as_str() {
        "stats" => {
            let archive = load_archive(&archive_path, nodes)?;
            let stats = archive.stats();
            println!("total_records={}", stats.total_records);
            println!(
                "distinct_solution_classes={}",
                stats.distinct_solution_classes
            );
            for (clues, count) in stats.by_clue_count {
                println!("clues_{clues}={count}");
            }
        }
        "verify" => {
            let archive = load_archive(&archive_path, nodes)?;
            println!(
                "verified_records={} archive={}",
                archive.records().len(),
                archive_path.display()
            );
        }
        "import" => {
            let source_path = source_path.ok_or_else(|| usage().to_owned())?;
            let imported = MinimalPuzzleArchive::load(&source_path, nodes)
                .map_err(|error| format!("source archive: {error}"))?;
            let mut archive = load_or_new(&archive_path, nodes)?;
            let inserted = archive.merge_verified(&imported);
            archive
                .save_atomic(&archive_path)
                .map_err(|error| error.to_string())?;
            println!(
                "imported={inserted} total={} archive={}",
                archive.records().len(),
                archive_path.display()
            );
        }
        "export" => {
            let output = output_path.ok_or_else(|| usage().to_owned())?;
            let archive = load_archive(&archive_path, nodes)?;
            archive
                .save_atomic(&output)
                .map_err(|error| error.to_string())?;
            println!(
                "exported={} archive={}",
                archive.records().len(),
                output.display()
            );
        }
        "seed-fast-bases" => seed_fast_bases(&archive_path, nodes)?,
        _ => return Err(format!("unknown archive command {command:?}\n{}", usage())),
    }
    Ok(())
}

fn load_archive(path: &Path, nodes: u64) -> Result<MinimalPuzzleArchive, String> {
    MinimalPuzzleArchive::load(path, nodes).map_err(|error| error.to_string())
}

fn load_or_new(path: &Path, nodes: u64) -> Result<MinimalPuzzleArchive, String> {
    if path.exists() {
        load_archive(path, nodes)
    } else {
        Ok(MinimalPuzzleArchive::new())
    }
}

fn seed_fast_bases(archive_path: &Path, nodes: u64) -> Result<(), String> {
    let mut archive = load_or_new(archive_path, nodes)?;
    let mut inserted = 0usize;
    for (puzzle, solution) in verified_17_clue_bases() {
        let analysis =
            analyze_minimality(puzzle, solution, nodes).map_err(|error| error.to_string())?;
        let was_inserted = archive
            .insert_analysis(
                &analysis,
                Provenance {
                    source: "bundled-fast-diverse-seeds".into(),
                    algorithm: "verified_17_clue_bases".into(),
                    seed: None,
                    parent_id: None,
                    operation: Some("seed-verified-archive".into()),
                    search_depth: 0,
                },
                nodes,
            )
            .map_err(|error| error.to_string())?;
        inserted += usize::from(was_inserted);
    }
    archive
        .save_atomic(archive_path)
        .map_err(|error| error.to_string())?;
    println!(
        "seeded={inserted} total={} clues_17={} archive={}",
        archive.records().len(),
        archive.count(17),
        archive_path.display()
    );
    Ok(())
}

pub(crate) fn usage() -> &'static str {
    "usage: sudoku archive <stats|verify|import|export|seed-fast-bases> [--archive PATH] [--source PATH] [--output PATH] [--nodes N]"
}
