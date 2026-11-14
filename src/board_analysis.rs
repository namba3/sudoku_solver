pub(super) struct BoardAnalysis {
    pub(super) conflicts: [[bool; 9]; 9],
    pub(super) candidate_masks: [[u16; 9]; 9],
    pub(super) hidden_single_masks: [[u16; 9]; 9],
    pub(super) no_candidates: [[bool; 9]; 9],
}

#[derive(Clone, Copy)]
pub(super) struct CandidateFilterOptions {
    pub(super) locked_candidates: bool,
    pub(super) naked_subsets: bool,
    pub(super) hidden_subsets: bool,
    pub(super) fish: bool,
    pub(super) xy_wing: bool,
    pub(super) skyscraper: bool,
    pub(super) two_string_kite: bool,
    pub(super) w_wing: bool,
    pub(super) iterations: usize,
}

impl Default for CandidateFilterOptions {
    fn default() -> Self {
        Self {
            locked_candidates: false,
            naked_subsets: false,
            hidden_subsets: false,
            fish: false,
            xy_wing: false,
            skyscraper: false,
            two_string_kite: false,
            w_wing: false,
            iterations: 1,
        }
    }
}

impl CandidateFilterOptions {
    pub(super) fn any_enabled(self) -> bool {
        self.locked_candidates
            || self.naked_subsets
            || self.hidden_subsets
            || self.fish
            || self.xy_wing
            || self.skyscraper
            || self.two_string_kite
            || self.w_wing
    }
}

pub(super) struct CandidateFilterResult {
    pub(super) candidate_masks: [[u16; 9]; 9],
    pub(super) eliminated_candidate_masks: [[u16; 9]; 9],
    pub(super) hidden_single_masks: [[u16; 9]; 9],
    pub(super) contradiction: bool,
}

impl BoardAnalysis {
    pub(super) fn new(board: &sudoku_solver::Matrix) -> Self {
        const ALL_CANDIDATES: u16 = (1 << 9) - 1;

        let mut row_masks = [0u16; 9];
        let mut column_masks = [0u16; 9];
        let mut block_masks = [0u16; 9];
        let mut row_duplicates = [0u16; 9];
        let mut column_duplicates = [0u16; 9];
        let mut block_duplicates = [0u16; 9];

        for y in 0..9 {
            for x in 0..9 {
                let value = board[y][x];
                if !(1..=9).contains(&value) {
                    continue;
                }

                let bit = 1 << (value - 1);
                let block = (y / 3) * 3 + x / 3;
                if row_masks[y] & bit != 0 {
                    row_duplicates[y] |= bit;
                }
                if column_masks[x] & bit != 0 {
                    column_duplicates[x] |= bit;
                }
                if block_masks[block] & bit != 0 {
                    block_duplicates[block] |= bit;
                }
                row_masks[y] |= bit;
                column_masks[x] |= bit;
                block_masks[block] |= bit;
            }
        }

        let mut conflicts = [[false; 9]; 9];
        let mut candidate_masks = [[0u16; 9]; 9];
        let mut no_candidates = [[false; 9]; 9];
        for y in 0..9 {
            for x in 0..9 {
                let value = board[y][x];
                let block = (y / 3) * 3 + x / 3;
                if (1..=9).contains(&value) {
                    let bit = 1 << (value - 1);
                    conflicts[y][x] = row_duplicates[y] & bit != 0
                        || column_duplicates[x] & bit != 0
                        || block_duplicates[block] & bit != 0;
                } else {
                    let candidates =
                        ALL_CANDIDATES & !(row_masks[y] | column_masks[x] | block_masks[block]);
                    candidate_masks[y][x] = candidates;
                    no_candidates[y][x] = candidates == 0;
                }
            }
        }

        let hidden_single_masks = find_hidden_singles(candidate_masks);

        Self {
            conflicts,
            candidate_masks,
            hidden_single_masks,
            no_candidates,
        }
    }

    pub(super) fn filter_candidates(
        &self,
        options: CandidateFilterOptions,
    ) -> CandidateFilterResult {
        filter_candidates(self.candidate_masks, options)
    }

    pub(super) fn conflict_count(&self) -> usize {
        self.conflicts
            .iter()
            .flatten()
            .filter(|&&cell| cell)
            .count()
    }

    pub(super) fn no_candidate_count(&self) -> usize {
        self.no_candidates
            .iter()
            .flatten()
            .filter(|&&cell| cell)
            .count()
    }
}

fn filter_candidates(
    original: [[u16; 9]; 9],
    options: CandidateFilterOptions,
) -> CandidateFilterResult {
    let mut candidates = original;
    if options.any_enabled() {
        for _ in 0..options.iterations {
            let round_candidates = candidates;
            let mut eliminations = [[0u16; 9]; 9];
            if options.locked_candidates {
                collect_locked_candidate_eliminations(&round_candidates, &mut eliminations);
            }
            for unit in candidate_units() {
                if options.naked_subsets {
                    collect_naked_subset_eliminations(&round_candidates, &unit, &mut eliminations);
                }
                if options.hidden_subsets {
                    collect_hidden_subset_eliminations(&round_candidates, &unit, &mut eliminations);
                }
            }
            if options.fish {
                collect_fish_eliminations(&round_candidates, false, &mut eliminations);
                collect_fish_eliminations(&round_candidates, true, &mut eliminations);
            }
            if options.xy_wing {
                collect_xy_wing_eliminations(&round_candidates, &mut eliminations);
            }
            if options.skyscraper {
                collect_skyscraper_eliminations(&round_candidates, false, &mut eliminations);
                collect_skyscraper_eliminations(&round_candidates, true, &mut eliminations);
            }
            if options.two_string_kite {
                collect_two_string_kite_eliminations(&round_candidates, &mut eliminations);
            }
            if options.w_wing {
                collect_w_wing_eliminations(&round_candidates, &mut eliminations);
            }
            let mut next_candidates = round_candidates;
            apply_eliminations(&mut next_candidates, &eliminations);
            if next_candidates == round_candidates {
                break;
            }
            candidates = next_candidates;
        }
    }

    let mut eliminated_candidate_masks = [[0u16; 9]; 9];
    let mut contradiction = false;
    for row in 0..9 {
        for column in 0..9 {
            eliminated_candidate_masks[row][column] =
                original[row][column] & !candidates[row][column];
            contradiction |= original[row][column] != 0 && candidates[row][column] == 0;
        }
    }
    CandidateFilterResult {
        candidate_masks: candidates,
        eliminated_candidate_masks,
        hidden_single_masks: if options.any_enabled() {
            find_hidden_singles(candidates)
        } else {
            [[0; 9]; 9]
        },
        contradiction,
    }
}

fn collect_fish_eliminations(
    candidates: &[[u16; 9]; 9],
    transposed: bool,
    eliminations: &mut [[u16; 9]; 9],
) {
    for digit in 0..9 {
        let bit = 1 << digit;
        for base_units in 1u16..(1 << 9) {
            let fish_size = base_units.count_ones();
            if !(2..=4).contains(&fish_size) {
                continue;
            }

            let mut cover_units = 0u16;
            let mut valid = true;
            for base in 0..9 {
                if base_units & (1 << base) == 0 {
                    continue;
                }
                let mut positions = 0u16;
                for cover in 0..9 {
                    let mask = if transposed {
                        candidates[cover][base]
                    } else {
                        candidates[base][cover]
                    };
                    if mask & bit != 0 {
                        positions |= 1 << cover;
                    }
                }
                if positions == 0 {
                    valid = false;
                    break;
                }
                cover_units |= positions;
                if cover_units.count_ones() > fish_size {
                    valid = false;
                    break;
                }
            }
            if !valid || cover_units.count_ones() != fish_size {
                continue;
            }

            for base in 0..9 {
                if base_units & (1 << base) != 0 {
                    continue;
                }
                for cover in 0..9 {
                    if cover_units & (1 << cover) == 0 {
                        continue;
                    }
                    if transposed {
                        eliminations[cover][base] |= bit;
                    } else {
                        eliminations[base][cover] |= bit;
                    }
                }
            }
        }
    }
}

fn collect_xy_wing_eliminations(candidates: &[[u16; 9]; 9], eliminations: &mut [[u16; 9]; 9]) {
    let peer_masks = cell_peer_masks();

    for pivot in 0..81 {
        let pivot_row = pivot / 9;
        let pivot_column = pivot % 9;
        let pivot_candidates = candidates[pivot_row][pivot_column];
        if pivot_candidates.count_ones() != 2 {
            continue;
        }

        for wing_a in 0..81 {
            if peer_masks[pivot] & (1u128 << wing_a) == 0 {
                continue;
            }
            let wing_a_candidates = candidates[wing_a / 9][wing_a % 9];
            if wing_a_candidates.count_ones() != 2 {
                continue;
            }
            let pivot_digit_a = wing_a_candidates & pivot_candidates;
            let wing_digit_a = wing_a_candidates & !pivot_candidates;
            if pivot_digit_a.count_ones() != 1 || wing_digit_a.count_ones() != 1 {
                continue;
            }

            for wing_b in 0..81 {
                if wing_b == wing_a || peer_masks[pivot] & (1u128 << wing_b) == 0 {
                    continue;
                }
                let wing_b_candidates = candidates[wing_b / 9][wing_b % 9];
                if wing_b_candidates.count_ones() != 2 {
                    continue;
                }
                let pivot_digit_b = wing_b_candidates & pivot_candidates;
                let wing_digit_b = wing_b_candidates & !pivot_candidates;
                if pivot_digit_b.count_ones() != 1
                    || pivot_digit_b == pivot_digit_a
                    || wing_digit_b.count_ones() != 1
                    || wing_digit_a != wing_digit_b
                {
                    continue;
                }

                let common_peers = peer_masks[wing_a] & peer_masks[wing_b];
                let mut targets = common_peers;
                while targets != 0 {
                    let target = targets.trailing_zeros() as usize;
                    targets &= targets - 1;
                    eliminations[target / 9][target % 9] |= wing_digit_a;
                }
            }
        }
    }
}

fn collect_skyscraper_eliminations(
    candidates: &[[u16; 9]; 9],
    transposed: bool,
    eliminations: &mut [[u16; 9]; 9],
) {
    let peer_masks = cell_peer_masks();
    for digit in 0..9 {
        let bit = 1 << digit;
        for first_base in 0..9 {
            let first_positions =
                candidate_positions_in_line(candidates, first_base, bit, transposed);
            if first_positions.count_ones() != 2 {
                continue;
            }
            for second_base in first_base + 1..9 {
                let second_positions =
                    candidate_positions_in_line(candidates, second_base, bit, transposed);
                if second_positions.count_ones() != 2 {
                    continue;
                }
                let shared_positions = first_positions & second_positions;
                if shared_positions.count_ones() != 1 {
                    continue;
                }

                let first_endpoint = first_positions & !shared_positions;
                let second_endpoint = second_positions & !shared_positions;
                if first_endpoint.count_ones() != 1 || second_endpoint.count_ones() != 1 {
                    continue;
                }
                let first_offset = first_endpoint.trailing_zeros() as usize;
                let second_offset = second_endpoint.trailing_zeros() as usize;
                let (first_row, first_column) = if transposed {
                    (first_offset, first_base)
                } else {
                    (first_base, first_offset)
                };
                let (second_row, second_column) = if transposed {
                    (second_offset, second_base)
                } else {
                    (second_base, second_offset)
                };
                collect_common_peer_elimination(
                    first_row * 9 + first_column,
                    second_row * 9 + second_column,
                    bit,
                    &peer_masks,
                    eliminations,
                );
            }
        }
    }
}

fn candidate_positions_in_line(
    candidates: &[[u16; 9]; 9],
    line: usize,
    digit: u16,
    transposed: bool,
) -> u16 {
    (0..9).fold(0u16, |positions, offset| {
        let mask = if transposed {
            candidates[offset][line]
        } else {
            candidates[line][offset]
        };
        if mask & digit != 0 {
            positions | (1 << offset)
        } else {
            positions
        }
    })
}

fn collect_two_string_kite_eliminations(
    candidates: &[[u16; 9]; 9],
    eliminations: &mut [[u16; 9]; 9],
) {
    let peer_masks = cell_peer_masks();
    for digit in 0..9 {
        let bit = 1 << digit;
        for row in 0..9 {
            let row_positions = candidate_positions_in_line(candidates, row, bit, false);
            if row_positions.count_ones() != 2 {
                continue;
            }
            for column in 0..9 {
                let column_positions = candidate_positions_in_line(candidates, column, bit, true);
                if column_positions.count_ones() != 2 {
                    continue;
                }

                for row_endpoint_column in 0..9 {
                    if row_positions & (1 << row_endpoint_column) == 0 {
                        continue;
                    }
                    for column_endpoint_row in 0..9 {
                        if column_positions & (1 << column_endpoint_row) == 0 {
                            continue;
                        }
                        let row_link_cell = row * 9 + row_endpoint_column;
                        let column_link_cell = column_endpoint_row * 9 + column;
                        if row_link_cell == column_link_cell
                            || (row / 3, row_endpoint_column / 3)
                                != (column_endpoint_row / 3, column / 3)
                        {
                            continue;
                        }

                        let row_far_column =
                            (row_positions & !(1 << row_endpoint_column)).trailing_zeros() as usize;
                        let column_far_row = (column_positions & !(1 << column_endpoint_row))
                            .trailing_zeros() as usize;
                        collect_common_peer_elimination(
                            row * 9 + row_far_column,
                            column_far_row * 9 + column,
                            bit,
                            &peer_masks,
                            eliminations,
                        );
                    }
                }
            }
        }
    }
}

fn collect_w_wing_eliminations(candidates: &[[u16; 9]; 9], eliminations: &mut [[u16; 9]; 9]) {
    let peer_masks = cell_peer_masks();
    let units = candidate_units();
    let mut strong_links = [[(0usize, 0usize); 27]; 9];
    let mut strong_link_counts = [0usize; 9];

    for digit in 0..9 {
        let bit = 1 << digit;
        for unit in &units {
            let mut link_cells = [0usize; 2];
            let mut link_count = 0;
            for &(row, column) in unit {
                if candidates[row][column] & bit == 0 {
                    continue;
                }
                if link_count < 2 {
                    link_cells[link_count] = row * 9 + column;
                }
                link_count += 1;
            }
            if link_count == 2 {
                let index = strong_link_counts[digit];
                strong_links[digit][index] = (link_cells[0], link_cells[1]);
                strong_link_counts[digit] += 1;
            }
        }
    }

    for first in 0..81 {
        let first_candidates = candidates[first / 9][first % 9];
        if first_candidates.count_ones() != 2 {
            continue;
        }
        for second in first + 1..81 {
            if candidates[second / 9][second % 9] != first_candidates {
                continue;
            }
            let mut digits = first_candidates;
            while digits != 0 {
                let strong_digit = digits & digits.wrapping_neg();
                digits &= digits - 1;
                let elimination_digit = first_candidates & !strong_digit;

                let digit_index = strong_digit.trailing_zeros() as usize;
                for link_index in 0..strong_link_counts[digit_index] {
                    let (link_first, link_second) = strong_links[digit_index][link_index];
                    let direct = peer_masks[first] & (1u128 << link_first) != 0
                        && peer_masks[second] & (1u128 << link_second) != 0;
                    let reversed = peer_masks[first] & (1u128 << link_second) != 0
                        && peer_masks[second] & (1u128 << link_first) != 0;
                    if direct || reversed {
                        collect_common_peer_elimination(
                            first,
                            second,
                            elimination_digit,
                            &peer_masks,
                            eliminations,
                        );
                    }
                }
            }
        }
    }
}

fn collect_common_peer_elimination(
    first: usize,
    second: usize,
    digit: u16,
    peer_masks: &[u128; 81],
    eliminations: &mut [[u16; 9]; 9],
) {
    let mut targets = peer_masks[first] & peer_masks[second];
    while targets != 0 {
        let target = targets.trailing_zeros() as usize;
        targets &= targets - 1;
        eliminations[target / 9][target % 9] |= digit;
    }
}

fn apply_eliminations(candidates: &mut [[u16; 9]; 9], eliminations: &[[u16; 9]; 9]) {
    for row in 0..9 {
        for column in 0..9 {
            candidates[row][column] &= !eliminations[row][column];
        }
    }
}

fn cell_peer_masks() -> [u128; 81] {
    std::array::from_fn(|cell| {
        let row = cell / 9;
        let column = cell % 9;
        let block = (row / 3) * 3 + column / 3;
        (0..81).fold(0u128, |peers, other| {
            let other_row = other / 9;
            let other_column = other % 9;
            let other_block = (other_row / 3) * 3 + other_column / 3;
            if other != cell && (other_row == row || other_column == column || other_block == block)
            {
                peers | (1u128 << other)
            } else {
                peers
            }
        })
    })
}

fn candidate_units() -> [[(usize, usize); 9]; 27] {
    std::array::from_fn(|unit_index| {
        std::array::from_fn(|offset| match unit_index {
            0..=8 => (unit_index, offset),
            9..=17 => (offset, unit_index - 9),
            _ => {
                let block = unit_index - 18;
                (block / 3 * 3 + offset / 3, block % 3 * 3 + offset % 3)
            }
        })
    })
}

fn collect_locked_candidate_eliminations(
    candidates: &[[u16; 9]; 9],
    eliminations: &mut [[u16; 9]; 9],
) {
    for block in 0..9 {
        let block_row = block / 3 * 3;
        let block_column = block % 3 * 3;
        for digit in 0..9 {
            let bit = 1 << digit;
            let mut count = 0;
            let mut only_row = None;
            let mut only_column = None;
            let mut same_row = true;
            let mut same_column = true;

            for row in block_row..block_row + 3 {
                for column in block_column..block_column + 3 {
                    if candidates[row][column] & bit == 0 {
                        continue;
                    }
                    count += 1;
                    same_row &= only_row.is_none_or(|candidate_row| candidate_row == row);
                    same_column &=
                        only_column.is_none_or(|candidate_column| candidate_column == column);
                    only_row.get_or_insert(row);
                    only_column.get_or_insert(column);
                }
            }

            if count > 0 && same_row {
                let row = only_row.expect("a candidate row exists when count is nonzero");
                for column in 0..9 {
                    if column / 3 != block_column / 3 {
                        eliminations[row][column] |= bit;
                    }
                }
            }
            if count > 0 && same_column {
                let column = only_column.expect("a candidate column exists when count is nonzero");
                for row in 0..9 {
                    if row / 3 != block_row / 3 {
                        eliminations[row][column] |= bit;
                    }
                }
            }
        }
    }

    for row in 0..9 {
        for digit in 0..9 {
            let bit = 1 << digit;
            let mut count = 0;
            let mut only_stack = None;
            let mut same_stack = true;
            for column in 0..9 {
                if candidates[row][column] & bit == 0 {
                    continue;
                }
                count += 1;
                same_stack &= only_stack.is_none_or(|stack| stack == column / 3);
                only_stack.get_or_insert(column / 3);
            }
            if count > 0 && same_stack {
                let stack = only_stack.expect("a candidate stack exists when count is nonzero");
                for other_row in (row / 3 * 3)..(row / 3 * 3 + 3) {
                    if other_row != row {
                        for column in (stack * 3)..(stack * 3 + 3) {
                            eliminations[other_row][column] |= bit;
                        }
                    }
                }
            }
        }
    }

    for column in 0..9 {
        for digit in 0..9 {
            let bit = 1 << digit;
            let mut count = 0;
            let mut only_band = None;
            let mut same_band = true;
            for row in 0..9 {
                if candidates[row][column] & bit == 0 {
                    continue;
                }
                count += 1;
                same_band &= only_band.is_none_or(|band| band == row / 3);
                only_band.get_or_insert(row / 3);
            }
            if count > 0 && same_band {
                let band = only_band.expect("a candidate band exists when count is nonzero");
                for other_row in (band * 3)..(band * 3 + 3) {
                    for other_column in (column / 3 * 3)..(column / 3 * 3 + 3) {
                        if other_column != column {
                            eliminations[other_row][other_column] |= bit;
                        }
                    }
                }
            }
        }
    }
}

fn collect_naked_subset_eliminations(
    candidates: &[[u16; 9]; 9],
    cells: &[(usize, usize); 9],
    eliminations: &mut [[u16; 9]; 9],
) {
    for selected_cells in 1u16..(1 << 9) {
        let subset_size = selected_cells.count_ones();
        if !(2..=4).contains(&subset_size) {
            continue;
        }

        let mut subset_digits = 0u16;
        let mut valid = true;
        for (index, &(row, column)) in cells.iter().enumerate() {
            if selected_cells & (1 << index) == 0 {
                continue;
            }
            if candidates[row][column] == 0 {
                valid = false;
                break;
            }
            subset_digits |= candidates[row][column];
        }
        if !valid || subset_digits.count_ones() != subset_size {
            continue;
        }

        for (index, &(row, column)) in cells.iter().enumerate() {
            if selected_cells & (1 << index) == 0 {
                eliminations[row][column] |= subset_digits;
            }
        }
    }
}

fn collect_hidden_subset_eliminations(
    candidates: &[[u16; 9]; 9],
    cells: &[(usize, usize); 9],
    eliminations: &mut [[u16; 9]; 9],
) {
    for subset_digits in 1u16..(1 << 9) {
        let subset_size = subset_digits.count_ones();
        if !(2..=4).contains(&subset_size) {
            continue;
        }

        let mut selected_cells = 0u16;
        let mut valid = true;
        for digit in 0..9 {
            let bit = 1 << digit;
            if subset_digits & bit == 0 {
                continue;
            }
            let digit_cells =
                cells
                    .iter()
                    .enumerate()
                    .fold(0u16, |mask, (index, &(row, column))| {
                        if candidates[row][column] & bit != 0 {
                            mask | (1 << index)
                        } else {
                            mask
                        }
                    });
            if digit_cells == 0 {
                valid = false;
                break;
            }
            selected_cells |= digit_cells;
        }
        if !valid || selected_cells.count_ones() != subset_size {
            continue;
        }

        for (index, &(row, column)) in cells.iter().enumerate() {
            if selected_cells & (1 << index) != 0 {
                eliminations[row][column] |= candidates[row][column] & !subset_digits;
            }
        }
    }
}

fn find_hidden_singles(candidates: [[u16; 9]; 9]) -> [[u16; 9]; 9] {
    let mut hidden_singles = [[0u16; 9]; 9];
    for row in 0..9 {
        let cells = std::array::from_fn(|column| (row, column));
        mark_hidden_singles(&cells, &candidates, &mut hidden_singles);
    }
    for column in 0..9 {
        let cells = std::array::from_fn(|row| (row, column));
        mark_hidden_singles(&cells, &candidates, &mut hidden_singles);
    }
    for block in 0..9 {
        let cells =
            std::array::from_fn(|offset| (block / 3 * 3 + offset / 3, block % 3 * 3 + offset % 3));
        mark_hidden_singles(&cells, &candidates, &mut hidden_singles);
    }
    hidden_singles
}

fn mark_hidden_singles(
    cells: &[(usize, usize); 9],
    candidate_masks: &[[u16; 9]; 9],
    hidden_single_masks: &mut [[u16; 9]; 9],
) {
    for digit in 0..9 {
        let bit = 1 << digit;
        let mut only_cell = None;
        for &(row, column) in cells {
            if candidate_masks[row][column] & bit == 0 {
                continue;
            }
            if only_cell.replace((row, column)).is_some() {
                only_cell = None;
                break;
            }
        }
        if let Some((row, column)) = only_cell {
            hidden_single_masks[row][column] |= bit;
        }
    }
}

pub(super) fn candidate_digits(mask: u16) -> impl Iterator<Item = u8> {
    (1..=9).filter(move |digit| mask & (1 << (digit - 1)) != 0)
}

#[cfg(test)]
mod tests {
    use super::{
        BoardAnalysis, CandidateFilterOptions, apply_eliminations, candidate_digits,
        candidate_units, collect_hidden_subset_eliminations, collect_naked_subset_eliminations,
        filter_candidates,
    };
    use sudoku_solver::Matrix;

    const PUZZLE: Matrix = [
        [5, 3, 0, 0, 7, 0, 0, 0, 0],
        [6, 0, 0, 1, 9, 5, 0, 0, 0],
        [0, 9, 8, 0, 0, 0, 0, 6, 0],
        [8, 0, 0, 0, 6, 0, 0, 0, 3],
        [4, 0, 0, 8, 0, 3, 0, 0, 1],
        [7, 0, 0, 0, 2, 0, 0, 0, 6],
        [0, 6, 0, 0, 0, 0, 2, 8, 0],
        [0, 0, 0, 4, 1, 9, 0, 0, 5],
        [0, 0, 0, 0, 8, 0, 0, 7, 9],
    ];

    #[test]
    fn candidate_masks_match_the_public_solver_api() {
        let analysis = BoardAnalysis::new(&PUZZLE);

        for y in 0..9 {
            for x in 0..9 {
                assert_eq!(
                    candidate_digits(analysis.candidate_masks[y][x]).collect::<Vec<_>>(),
                    sudoku_solver::candidates_for(&PUZZLE, x, y),
                    "candidate mismatch at ({y}, {x})"
                );
            }
        }
    }

    #[test]
    fn marks_all_cells_in_duplicate_units() {
        let mut board = Matrix::default();
        board[0][0] = 5;
        board[0][1] = 5;
        board[3][3] = 6;
        board[4][3] = 6;
        board[6][6] = 7;
        board[7][7] = 7;

        let analysis = BoardAnalysis::new(&board);
        for (y, x) in [(0, 0), (0, 1), (3, 3), (4, 3), (6, 6), (7, 7)] {
            assert!(analysis.conflicts[y][x], "expected conflict at ({y}, {x})");
        }
        assert!(!analysis.conflicts[0][2]);
        assert_eq!(analysis.conflict_count(), 6);
    }

    #[test]
    fn detects_empty_cells_with_no_legal_digits() {
        let mut board = Matrix::default();
        board[0] = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        for (y, value) in [4, 6, 1, 2, 3, 5, 7, 8].into_iter().enumerate() {
            board[y + 1][0] = value;
        }
        board[1][1] = 9;

        let analysis = BoardAnalysis::new(&board);
        assert!(analysis.no_candidates[0][0]);
        assert_eq!(analysis.candidate_masks[0][0], 0);
        assert_eq!(analysis.no_candidate_count(), 1);
    }

    #[test]
    fn detects_hidden_single_digit_and_cell_even_with_other_candidates() {
        let board: Matrix = [
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [6, 0, 0, 1, 0, 0, 0, 0, 0],
            [0, 0, 8, 0, 4, 0, 0, 0, 0],
            [8, 0, 9, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [7, 1, 0, 9, 0, 0, 0, 5, 0],
            [0, 0, 0, 0, 3, 7, 0, 8, 4],
            [0, 0, 0, 0, 0, 0, 0, 3, 5],
            [0, 0, 0, 2, 8, 0, 0, 0, 0],
        ];

        let analysis = BoardAnalysis::new(&board);
        let target_digit = 8;
        let target_bit = 1 << (target_digit - 1);

        assert_ne!(analysis.hidden_single_masks[7][1] & target_bit, 0);
        assert!(analysis.candidate_masks[7][1].count_ones() > 1);
        assert_eq!(
            (0..9)
                .filter(|&column| analysis.candidate_masks[7][column] & target_bit != 0)
                .count(),
            1
        );
    }

    #[test]
    fn detects_hidden_singles_in_columns_and_blocks() {
        let column_board: Matrix = [
            [0, 0, 0, 6, 0, 0, 0, 0, 2],
            [0, 0, 2, 1, 9, 0, 0, 0, 8],
            [0, 0, 0, 0, 0, 0, 5, 0, 0],
            [0, 0, 0, 0, 0, 1, 0, 0, 0],
            [4, 2, 6, 0, 0, 0, 0, 0, 0],
            [0, 1, 0, 0, 2, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 1, 9, 6, 0, 5],
            [0, 0, 0, 0, 0, 6, 0, 0, 0],
        ];
        let block_board: Matrix = [
            [0, 0, 0, 0, 0, 8, 0, 0, 0],
            [0, 0, 0, 1, 0, 0, 0, 0, 0],
            [0, 0, 0, 0, 0, 0, 0, 6, 0],
            [8, 0, 0, 0, 6, 1, 4, 0, 0],
            [0, 2, 0, 0, 5, 3, 0, 0, 0],
            [7, 0, 0, 0, 0, 4, 0, 0, 0],
            [0, 0, 0, 5, 0, 0, 2, 8, 0],
            [0, 0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 5, 2, 8, 0, 0, 0, 0],
        ];

        for (board, row, column, digit) in [(&column_board, 3, 4, 6), (&block_board, 5, 4, 2)] {
            let analysis = BoardAnalysis::new(board);
            let bit = 1 << (digit - 1);

            assert_ne!(analysis.hidden_single_masks[row][column] & bit, 0);
            assert!(analysis.candidate_masks[row][column].count_ones() > 1);
        }
    }

    #[test]
    fn locked_candidates_remove_digits_from_intersecting_rows_and_blocks() {
        let digit_five = 1 << 4;
        let other_candidate = 1 << 0;

        let mut pointing_candidates = [[0u16; 9]; 9];
        pointing_candidates[0][0] = digit_five;
        pointing_candidates[0][1] = digit_five;
        pointing_candidates[0][3] = digit_five | other_candidate;
        let filtered_pointing = filter_candidates(
            pointing_candidates,
            CandidateFilterOptions {
                locked_candidates: true,
                ..CandidateFilterOptions::default()
            },
        )
        .candidate_masks;
        assert_eq!(filtered_pointing[0][3] & digit_five, 0);
        assert_ne!(filtered_pointing[0][3] & other_candidate, 0);

        let mut claiming_candidates = [[0u16; 9]; 9];
        claiming_candidates[0][0] = digit_five;
        claiming_candidates[0][1] = digit_five;
        claiming_candidates[1][2] = digit_five | other_candidate;
        let filtered_claiming = filter_candidates(
            claiming_candidates,
            CandidateFilterOptions {
                locked_candidates: true,
                ..CandidateFilterOptions::default()
            },
        )
        .candidate_masks;
        assert_eq!(filtered_claiming[1][2] & digit_five, 0);
        assert_ne!(filtered_claiming[1][2] & other_candidate, 0);
    }

    #[test]
    fn candidate_filter_iteration_count_defaults_to_one_and_limits_passes() {
        let digit_five = 1 << 4;
        let other_candidate = 1 << 0;
        let mut candidates = [[0u16; 9]; 9];
        candidates[0][0] = digit_five;
        candidates[0][1] = digit_five;
        candidates[0][3] = digit_five | other_candidate;

        assert_eq!(CandidateFilterOptions::default().iterations, 1);

        let unchanged = filter_candidates(
            candidates,
            CandidateFilterOptions {
                locked_candidates: true,
                iterations: 0,
                ..CandidateFilterOptions::default()
            },
        );
        assert_eq!(unchanged.candidate_masks, candidates);

        let one_pass = filter_candidates(
            candidates,
            CandidateFilterOptions {
                locked_candidates: true,
                ..CandidateFilterOptions::default()
            },
        );
        assert_eq!(one_pass.candidate_masks[0][3] & digit_five, 0);
        assert_ne!(one_pass.candidate_masks[0][3] & other_candidate, 0);
    }

    #[test]
    fn one_round_does_not_research_xy_wings_after_a_subset_elimination() {
        let digits_one_two = (1 << 0) | (1 << 1);
        let digit_three = 1 << 2;
        let digit_seven = 1 << 6;
        let digit_eight = 1 << 7;
        let digit_nine = 1 << 8;
        let mut candidates = [[0u16; 9]; 9];
        candidates[0][0] = digits_one_two;
        candidates[0][1] = digits_one_two;
        candidates[0][3] = digits_one_two | digit_eight | digit_nine;
        candidates[0][6] = digit_three | digit_nine;
        candidates[4][3] = digit_seven | digit_eight;
        candidates[4][6] = digit_seven | digit_nine;

        let one_round = filter_candidates(
            candidates,
            CandidateFilterOptions {
                naked_subsets: true,
                xy_wing: true,
                iterations: 1,
                ..CandidateFilterOptions::default()
            },
        );
        assert_eq!(one_round.candidate_masks[0][3], digit_eight | digit_nine);
        assert_ne!(one_round.candidate_masks[0][6] & digit_nine, 0);

        let two_rounds = filter_candidates(
            candidates,
            CandidateFilterOptions {
                naked_subsets: true,
                xy_wing: true,
                iterations: 2,
                ..CandidateFilterOptions::default()
            },
        );
        assert_eq!(two_rounds.candidate_masks[0][3], digit_eight | digit_nine);
        assert_eq!(two_rounds.candidate_masks[0][6] & digit_nine, 0);
    }

    #[test]
    fn fish_filter_handles_sizes_two_through_four_in_both_directions() {
        let digit = 1 << 4;
        let other = 1;

        for size in 2..=4 {
            for transposed in [false, true] {
                let mut candidates = [[0u16; 9]; 9];
                for base in 0..size {
                    for cover in [base, (base + 1) % size] {
                        let (row, column) = if transposed {
                            (cover, base)
                        } else {
                            (base, cover)
                        };
                        candidates[row][column] |= digit;
                    }
                }
                for cover in 0..size {
                    let (row, column) = if transposed { (cover, 8) } else { (8, cover) };
                    candidates[row][column] = digit | other;
                }

                let filtered = filter_candidates(
                    candidates,
                    CandidateFilterOptions {
                        fish: true,
                        ..CandidateFilterOptions::default()
                    },
                );
                for cover in 0..size {
                    let (row, column) = if transposed { (cover, 8) } else { (8, cover) };
                    assert_eq!(filtered.candidate_masks[row][column] & digit, 0);
                    assert_ne!(filtered.candidate_masks[row][column] & other, 0);
                }
            }
        }
    }

    #[test]
    fn xy_wing_removes_its_shared_wing_candidate_from_common_peers() {
        let digit_two = 1 << 1;
        let digit_five = 1 << 4;
        let digit_eight = 1 << 7;
        let other = 1 << 8;
        let mut candidates = [[0u16; 9]; 9];

        candidates[4][4] = digit_two | digit_five;
        candidates[4][1] = digit_two | digit_eight;
        candidates[1][4] = digit_five | digit_eight;
        candidates[1][1] = digit_eight | other;
        candidates[0][8] = digit_eight | other;

        let filtered = filter_candidates(
            candidates,
            CandidateFilterOptions {
                xy_wing: true,
                ..CandidateFilterOptions::default()
            },
        );

        assert_eq!(filtered.candidate_masks[1][1] & digit_eight, 0);
        assert_ne!(filtered.candidate_masks[1][1] & other, 0);
        assert_ne!(filtered.candidate_masks[0][8] & digit_eight, 0);
    }

    #[test]
    fn skyscraper_eliminates_from_common_peers_in_rows_and_columns() {
        let digit = 1 << 4;
        let other = 1;

        for transposed in [false, true] {
            let mut candidates = [[0u16; 9]; 9];
            for (row, column) in [(0, 1), (0, 7), (3, 1), (3, 8)] {
                let (row, column) = if transposed {
                    (column, row)
                } else {
                    (row, column)
                };
                candidates[row][column] = digit;
            }
            let (target_row, target_column) = if transposed { (8, 1) } else { (1, 8) };
            candidates[target_row][target_column] = digit | other;

            let filtered = filter_candidates(
                candidates,
                CandidateFilterOptions {
                    skyscraper: true,
                    ..CandidateFilterOptions::default()
                },
            );

            assert_eq!(
                filtered.candidate_masks[target_row][target_column] & digit,
                0
            );
            assert_ne!(
                filtered.candidate_masks[target_row][target_column] & other,
                0
            );
        }
    }

    #[test]
    fn two_string_kite_eliminates_from_cells_seeing_both_far_endpoints() {
        let digit = 1 << 4;
        let other = 1;
        let mut candidates = [[0u16; 9]; 9];
        candidates[0][3] = digit;
        candidates[0][7] = digit;
        candidates[1][4] = digit;
        candidates[5][4] = digit;
        candidates[5][7] = digit | other;

        let filtered = filter_candidates(
            candidates,
            CandidateFilterOptions {
                two_string_kite: true,
                ..CandidateFilterOptions::default()
            },
        );

        assert_eq!(filtered.candidate_masks[5][7] & digit, 0);
        assert_ne!(filtered.candidate_masks[5][7] & other, 0);
    }

    #[test]
    fn w_wing_uses_a_strong_link_to_eliminate_the_other_pair_digit() {
        let digit_three = 1 << 2;
        let digit_seven = 1 << 6;
        let mut candidates = [[0u16; 9]; 9];
        candidates[0][0] = digit_three | digit_seven;
        candidates[4][4] = digit_three | digit_seven;
        candidates[2][0] = digit_seven | (1 << 0);
        candidates[2][4] = digit_seven | (1 << 1);
        candidates[0][4] = digit_three | (1 << 8);

        let filtered = filter_candidates(
            candidates,
            CandidateFilterOptions {
                w_wing: true,
                ..CandidateFilterOptions::default()
            },
        );

        assert_eq!(filtered.candidate_masks[0][4] & digit_three, 0);
        assert_ne!(filtered.candidate_masks[0][4] & (1 << 8), 0);
    }

    #[test]
    fn naked_subsets_of_sizes_two_through_four_remove_their_digits_from_peers() {
        let cells = candidate_units()[0];
        let extra_digit = 1 << 8;

        for subset_size in 2..=4 {
            let subset_digits = (1u16 << subset_size) - 1;
            let mut candidates = [[0u16; 9]; 9];
            for &(row, column) in cells.iter().take(subset_size as usize) {
                candidates[row][column] = subset_digits;
            }
            let (row, column) = cells[subset_size as usize];
            candidates[row][column] = subset_digits | extra_digit;

            let mut eliminations = [[0u16; 9]; 9];
            collect_naked_subset_eliminations(&candidates, &cells, &mut eliminations);
            apply_eliminations(&mut candidates, &eliminations);

            assert_eq!(candidates[row][column], extra_digit);
        }
    }

    #[test]
    fn hidden_subsets_of_sizes_two_through_four_restrict_their_cells() {
        let cells = candidate_units()[0];
        let extra_digit = 1 << 8;

        for subset_size in 2..=4 {
            let subset_digits = (1u16 << subset_size) - 1;
            let mut candidates = [[0u16; 9]; 9];
            for &(row, column) in cells.iter().take(subset_size as usize) {
                candidates[row][column] = subset_digits | extra_digit;
            }
            for &(row, column) in cells.iter().skip(subset_size as usize) {
                candidates[row][column] = extra_digit;
            }

            let mut eliminations = [[0u16; 9]; 9];
            collect_hidden_subset_eliminations(&candidates, &cells, &mut eliminations);
            apply_eliminations(&mut candidates, &eliminations);

            for &(row, column) in cells.iter().take(subset_size as usize) {
                assert_eq!(candidates[row][column], subset_digits);
            }
        }
    }

    #[test]
    fn hidden_subsets_ignore_digits_with_no_candidate_positions() {
        let cells = candidate_units()[0];
        let digit_one = 1;
        let absent_digit = 1 << 1;
        let digit_nine = 1 << 8;
        let mut candidates = [[0u16; 9]; 9];
        candidates[0][0] = digit_one | digit_nine;
        candidates[0][1] = digit_one | digit_nine;
        for &(row, column) in cells.iter().skip(2) {
            candidates[row][column] = digit_nine;
        }

        let mut eliminations = [[0u16; 9]; 9];
        collect_hidden_subset_eliminations(&candidates, &cells, &mut eliminations);
        apply_eliminations(&mut candidates, &eliminations);

        assert_eq!(candidates[0][0], digit_one | digit_nine);
        assert_eq!(candidates[0][1], digit_one | digit_nine);
        assert_eq!(candidates[0][0] & absent_digit, 0);
    }
}
