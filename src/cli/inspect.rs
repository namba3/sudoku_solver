use super::super::puzzle_text::parse_board_text;
use clap::{ArgGroup, Args};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use sudoku_solver::archive::analyze_minimality_from_puzzle;
use sudoku_solver::{candidates_for, Matrix, SearchEvent, SolutionSearch};

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("puzzle_input").required(true).args(["puzzle", "file"]))) ]
pub(super) struct InspectArgs {
    /// Puzzle text with 81 cells; line breaks are ignored and 0, . or _ means empty.
    #[arg(long, value_name = "PUZZLE_TEXT")]
    puzzle: Option<String>,
    /// Read puzzle text from a file; line breaks are ignored and 0, . or _ means empty.
    #[arg(long, value_name = "PATH")]
    file: Option<PathBuf>,
    /// Maximum solver nodes for solution enumeration.
    #[arg(long, default_value_t = 1_000_000)]
    nodes: u64,
    /// Stop after finding this many solutions; a capped result is reported as a lower bound.
    #[arg(long, default_value_t = 100_000)]
    max_solutions: usize,
    /// Maximum solver nodes for each minimality check.
    #[arg(long, default_value_t = 100_000)]
    minimality_nodes: u64,
    /// Skip clue-deletion checks after uniqueness is established.
    #[arg(long)]
    skip_minimality: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SolutionCount {
    Exact {
        count: usize,
        nodes: u64,
    },
    AtLeast {
        count: usize,
        nodes: u64,
        reason: &'static str,
    },
    Unknown {
        nodes: u64,
    },
}

pub(super) fn run(args: InspectArgs) -> Result<(), String> {
    if args.nodes == 0 || args.max_solutions == 0 || args.minimality_nodes == 0 {
        return Err("--nodes, --max-solutions, and --minimality-nodes must be positive".into());
    }
    let puzzle = match (args.puzzle, args.file) {
        (Some(encoded), None) => parse_board_text(&encoded).map_err(|error| error.to_string())?,
        (None, Some(path)) => {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            parse_board_text(&text).map_err(|error| format!("{}: {error}", path.display()))?
        }
        _ => return Err("provide exactly one of --puzzle or --file".into()),
    };
    let clue_count = puzzle
        .iter()
        .flatten()
        .filter(|&&value| (1..=9).contains(&value))
        .count();
    println!("clues={clue_count}");

    let mut search = match SolutionSearch::new(&puzzle) {
        Ok(search) => search,
        Err(_) => {
            println!("board=invalid reason=conflicting-values");
            println!("solutions=not-searched");
            println!("naked_singles=not-assessed");
            println!("hidden_singles=not-assessed");
            println!("minimality=not-applicable");
            return Ok(());
        }
    };

    let (naked_singles, hidden_singles, no_candidate_cells) = find_singles(&puzzle);
    let solution_count = count_solutions(&mut search, args.max_solutions, args.nodes);
    println!("givens=consistent no_candidate_cells={no_candidate_cells}");
    print_solution_count(solution_count);
    print_singles("naked_singles", &naked_singles);
    print_singles("hidden_singles", &hidden_singles);

    match solution_count {
        SolutionCount::Exact { count: 1, .. } if !args.skip_minimality => {
            let analysis = analyze_minimality_from_puzzle(puzzle, args.minimality_nodes)
                .map_err(|error| error.to_string())?;
            println!(
                "minimality={:?} minimality_nodes={}",
                analysis.status, analysis.evidence.total_nodes
            );
        }
        SolutionCount::Exact { count: 1, .. } => {
            println!("minimality=skipped");
        }
        SolutionCount::Exact { .. } => {
            println!("minimality=not-applicable reason=solution-count-is-not-one");
        }
        SolutionCount::AtLeast { .. } => {
            println!("minimality=not-assessed reason=solution-count-is-incomplete");
        }
        SolutionCount::Unknown { .. } => {
            println!("minimality=not-assessed reason=solution-count-is-unknown");
        }
    }
    Ok(())
}

fn count_solutions(
    search: &mut SolutionSearch,
    max_solutions: usize,
    max_nodes: u64,
) -> SolutionCount {
    let mut count = 0;
    loop {
        match search.next() {
            Some(SearchEvent::SolutionFound { .. }) => {
                count += 1;
                if count >= max_solutions {
                    return SolutionCount::AtLeast {
                        count,
                        nodes: search.explored_nodes(),
                        reason: "solution_limit",
                    };
                }
            }
            Some(SearchEvent::Progress { nodes, .. }) if nodes >= max_nodes => {
                return if count == 0 {
                    SolutionCount::Unknown { nodes }
                } else {
                    SolutionCount::AtLeast {
                        count,
                        nodes,
                        reason: "node_limit",
                    }
                };
            }
            Some(SearchEvent::Progress { .. }) => {}
            None => {
                return SolutionCount::Exact {
                    count,
                    nodes: search.explored_nodes(),
                };
            }
        }
    }
}

fn find_singles(puzzle: &Matrix) -> (Vec<(usize, usize, u8)>, Vec<(usize, usize, u8)>, usize) {
    let mut candidates = [[0u16; 9]; 9];
    let mut naked = Vec::new();
    let mut no_candidates = 0;
    for row in 0..9 {
        for column in 0..9 {
            if (1..=9).contains(&puzzle[row][column]) {
                continue;
            }
            let mask = candidates_for(puzzle, column, row)
                .into_iter()
                .fold(0u16, |mask, digit| mask | (1 << (digit - 1)));
            candidates[row][column] = mask;
            match mask.count_ones() {
                0 => no_candidates += 1,
                1 => naked.push((row, column, mask.trailing_zeros() as u8 + 1)),
                _ => {}
            }
        }
    }

    let mut hidden = BTreeSet::new();
    let mut inspect_unit = |cells: &[(usize, usize)]| {
        for digit in 1..=9 {
            let bit = 1 << (digit - 1);
            let mut locations = cells.iter().copied().filter(|&(row, column)| {
                !(1..=9).contains(&puzzle[row][column]) && candidates[row][column] & bit != 0
            });
            if let Some((row, column)) = locations.next() {
                if locations.next().is_none() {
                    hidden.insert((row, column, digit));
                }
            }
        }
    };
    for row in 0..9 {
        inspect_unit(&(0..9).map(|column| (row, column)).collect::<Vec<_>>());
    }
    for column in 0..9 {
        inspect_unit(&(0..9).map(|row| (row, column)).collect::<Vec<_>>());
    }
    for box_row in (0..9).step_by(3) {
        for box_column in (0..9).step_by(3) {
            let cells = (box_row..box_row + 3)
                .flat_map(|row| (box_column..box_column + 3).map(move |column| (row, column)))
                .collect::<Vec<_>>();
            inspect_unit(&cells);
        }
    }

    (naked, hidden.into_iter().collect(), no_candidates)
}

fn print_solution_count(result: SolutionCount) {
    match result {
        SolutionCount::Exact { count, nodes } => {
            let classification = match count {
                0 => "none",
                1 => "unique",
                _ => "multiple",
            };
            println!("solutions=exact:{count} classification={classification} nodes={nodes}");
        }
        SolutionCount::AtLeast {
            count,
            nodes,
            reason,
        } => {
            let classification = if count > 1 {
                "multiple"
            } else {
                "has-solution"
            };
            println!(
                "solutions=at-least:{count} classification={classification} reason={reason} nodes={nodes}"
            );
        }
        SolutionCount::Unknown { nodes } => {
            println!("solutions=unknown classification=unknown reason=node_limit nodes={nodes}");
        }
    }
}

fn print_singles(label: &str, singles: &[(usize, usize, u8)]) {
    let locations = singles
        .iter()
        .map(|&(row, column, digit)| format!("r{}c{}={digit}", row + 1, column + 1))
        .collect::<Vec<_>>()
        .join(",");
    println!("{label}={} [{}]", singles.len(), locations);
}

#[cfg(test)]
mod tests {
    use super::{count_solutions, find_singles, SolutionCount};
    use crate::puzzle_text::parse_board_text;
    use sudoku_solver::{Matrix, SolutionSearch};

    const UNIQUE_PUZZLE: &str =
        "530070000600195000098000060800060003400803001700020006060000280000419005000080079";

    fn parse(encoded: &str) -> Matrix {
        let mut board = [[0; 9]; 9];
        for (index, digit) in encoded.bytes().enumerate() {
            board[index / 9][index % 9] = digit - b'0';
        }
        board
    }

    #[test]
    fn solution_count_reports_exact_unique_and_bounded_multiple_results() {
        let unique = parse(UNIQUE_PUZZLE);
        let mut search = SolutionSearch::new(&unique).unwrap();
        assert!(matches!(
            count_solutions(&mut search, 10, 1_000_000),
            SolutionCount::Exact { count: 1, .. }
        ));

        let mut search = SolutionSearch::new(&[[0; 9]; 9]).unwrap();
        assert!(matches!(
            count_solutions(&mut search, 2, 1_000_000),
            SolutionCount::AtLeast { count: 2, .. }
        ));

        let mut unsatisfiable = [[0; 9]; 9];
        unsatisfiable[0][1..9].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        for (row, digit) in [4, 5, 2, 3, 6, 7, 8, 9].into_iter().enumerate() {
            unsatisfiable[row + 1][0] = digit;
        }
        let mut search = SolutionSearch::new(&unsatisfiable).unwrap();
        assert!(matches!(
            count_solutions(&mut search, 10, 1_000_000),
            SolutionCount::Exact { count: 0, .. }
        ));
    }

    #[test]
    fn singles_are_reported_as_unique_cell_digit_assignments() {
        let (naked, hidden, no_candidates) = find_singles(&parse(UNIQUE_PUZZLE));
        assert!(!naked.is_empty());
        assert!(!hidden.is_empty());
        assert_eq!(no_candidates, 0);
    }

    #[test]
    fn file_format_accepts_all_empty_markers_and_crlf() {
        let text = concat!(
            ".....5.8.\r\n",
            "...6.1.43\r\n",
            ".........\r\n",
            ".1.5.....\r\n",
            "...1.6...\r\n",
            "3.......5\r\n",
            "53.....61\r\n",
            "........4\r\n",
            "_________\r\n",
        );
        let board = parse_board_text(text).unwrap();
        assert_eq!(board[0], [0, 0, 0, 0, 0, 5, 0, 8, 0]);
        assert_eq!(board[8], [0; 9]);
    }

    #[test]
    fn file_format_rejects_wrong_shapes_and_characters() {
        assert!(parse_board_text(".........\n")
            .unwrap_err()
            .to_string()
            .contains("81 cells"));
        let mut rows = ["........."; 9].join("\n");
        rows.replace_range(0..1, "x");
        assert!(parse_board_text(&rows)
            .unwrap_err()
            .to_string()
            .contains("row 1, column 1"));
    }
}
