use crate::solver::{SearchEvent, SolutionSearch};
use crate::Matrix;

const UNIQUENESS_NODE_LIMIT: u64 = 100_000;

/// Generate a randomized puzzle with exactly one solution.
///
/// The seed makes output reproducible. Clues are removed only when the
/// solution iterator proves that the remaining board has exactly one
/// solution. A bounded uniqueness check keeps generation work predictable;
/// when its node limit is reached, the clue is retained.
pub fn generate_puzzle(seed: u64) -> Matrix {
    let mut rng = SplitMix64(seed);
    let mut puzzle = randomized_solution(&mut rng);

    let mut positions: Vec<usize> = (0..81).collect();
    rng.shuffle(&mut positions);
    for position in positions {
        let y = position / 9;
        let x = position % 9;
        let clue = puzzle[y][x];
        puzzle[y][x] = 0;
        if !has_exactly_one_solution(&puzzle) {
            puzzle[y][x] = clue;
        }
    }

    puzzle
}

fn has_exactly_one_solution(board: &Matrix) -> bool {
    let Ok(search) = SolutionSearch::new(board) else {
        return false;
    };

    let mut solutions = 0;
    for event in search {
        match event {
            SearchEvent::SolutionFound { .. } => {
                solutions += 1;
                if solutions > 1 {
                    return false;
                }
            }
            SearchEvent::Progress { nodes, .. } if nodes >= UNIQUENESS_NODE_LIMIT => {
                return false;
            }
            SearchEvent::Progress { .. } => {}
        }
    }
    solutions == 1
}

fn randomized_solution(rng: &mut SplitMix64) -> Matrix {
    let rows = grouped_permutation(rng);
    let columns = grouped_permutation(rng);
    let mut digits = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    rng.shuffle(&mut digits);

    let mut board = [[0; 9]; 9];
    for y in 0..9 {
        for x in 0..9 {
            let source_row = rows[y];
            let source_column = columns[x];
            let base_digit = (source_row * 3 + source_row / 3 + source_column) % 9;
            board[y][x] = digits[base_digit];
        }
    }

    if rng.next() & 1 == 1 {
        for y in 0..9 {
            for x in (y + 1)..9 {
                let value = board[y][x];
                board[y][x] = board[x][y];
                board[x][y] = value;
            }
        }
    }
    board
}

fn grouped_permutation(rng: &mut SplitMix64) -> [usize; 9] {
    let mut groups = [0, 1, 2];
    rng.shuffle(&mut groups);
    let mut result = [0; 9];
    for (group_index, group) in groups.into_iter().enumerate() {
        let mut within_group = [0, 1, 2];
        rng.shuffle(&mut within_group);
        for (offset, item) in within_group.into_iter().enumerate() {
            result[group_index * 3 + offset] = group * 3 + item;
        }
    }
    result
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for index in (1..values.len()).rev() {
            let other = (self.next() % (index as u64 + 1)) as usize;
            values.swap(index, other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::generate_puzzle;
    use crate::{SearchEvent, SolutionSearch};

    fn clue_count(board: &crate::Matrix) -> usize {
        board
            .iter()
            .flatten()
            .filter(|&&value| (1..=9).contains(&value))
            .count()
    }

    #[test]
    fn generation_is_reproducible_for_a_seed() {
        assert_eq!(generate_puzzle(42), generate_puzzle(42));
    }

    #[test]
    fn generated_puzzle_has_one_solution_and_removes_clues() {
        let puzzle = generate_puzzle(7);
        assert!(clue_count(&puzzle) < 81);

        let mut solution_count = 0;
        for event in SolutionSearch::new(&puzzle).unwrap() {
            if matches!(event, SearchEvent::SolutionFound { .. }) {
                solution_count += 1;
                if solution_count > 1 {
                    break;
                }
            }
        }
        assert_eq!(solution_count, 1);
    }
}
