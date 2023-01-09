use crate::Matrix;

/// Solve the Sudoku
pub fn solve(mtx: &mut Matrix) -> bool {
    let mut manager = StateManager::new();
    let mut empty_cells = Vec::new();

    for y in 0..9 {
        for x in 0..9 {
            let val = &mut mtx[y][x];
            if !(1..=9).contains(val) {
                empty_cells.push((x, y));
                *val = 0;
                continue;
            }
            if !manager.set(x, y, *val) {
                return false;
            }
        }
    }

    fill(mtx, &mut empty_cells, &mut manager)
}

/// Fill the Sudoku matrix with temporary placement method
fn fill(
    mtx: &mut Matrix,
    empty_cells: &mut Vec<(usize, usize)>,
    manager: &mut StateManager,
) -> bool {
    if empty_cells.is_empty() {
        return true;
    }

    // Select the cell with fewest number of candidates
    let (x, y) = {
        let min_idx = empty_cells
            .iter()
            .copied()
            .enumerate()
            .min_by_key(|&(_idx, (x, y))| manager.num_candidates(x, y))
            .map(|(idx, _)| idx)
            .unwrap();
        empty_cells.swap_remove(min_idx)
    };

    // Backtrack if the cell has no candidates
    if manager.num_candidates(x, y) <= 0 {
        empty_cells.push((x, y));
        return false;
    }

    // Temporarily place candidates and self-recurse
    for val in manager.candidates(x, y) {
        manager.set(x, y, val);
        mtx[y][x] = val;
        if fill(mtx, empty_cells, manager) {
            return true;
        }
        manager.remove(x, y, val);
    }

    // Backtrack if the temporary placement method fails
    empty_cells.push((x, y));
    mtx[y][x] = 0;
    false
}

/// A state manager that manages whether each cell in the matrix can have candidates
///
/// This uses bitwise operations to find candidates faster than using naive `for` statements.
struct StateManager {
    row: [u16; 9],
    col: [u16; 9],
    sub_mtx: [[u16; 3]; 3],
}
impl StateManager {
    pub const fn new() -> Self {
        Self {
            row: [0; 9],
            col: [0; 9],
            sub_mtx: [[0; 3]; 3],
        }
    }

    pub fn set(&mut self, x: usize, y: usize, val: u8) -> bool {
        if !self.is_settable(x, y, val) {
            return false;
        }

        let flag = Self::flag(val);
        self.row[y] |= flag;
        self.col[x] |= flag;
        self.sub_mtx[y / 3][x / 3] |= flag;

        true
    }

    pub fn remove(&mut self, x: usize, y: usize, val: u8) {
        let mask = !Self::flag(val);
        self.row[y] &= mask;
        self.col[x] &= mask;
        self.sub_mtx[y / 3][x / 3] &= mask;
    }

    pub const fn candidates(&self, x: usize, y: usize) -> Candidates {
        let bits = self.bits(x, y);
        Candidates::new(bits)
    }

    pub const fn num_candidates(&self, x: usize, y: usize) -> u8 {
        let bits = self.bits(x, y);
        9 - bits.count_ones() as u8
    }

    const fn flag(val: u8) -> u16 {
        1u16 << (val - 1)
    }

    const fn bits(&self, x: usize, y: usize) -> u16 {
        self.row[y] | self.col[x] | self.sub_mtx[y / 3][x / 3]
    }

    const fn is_settable(&self, x: usize, y: usize, val: u8) -> bool {
        let flag = Self::flag(val);
        let bits = self.bits(x, y);
        (bits & flag) == 0
    }
}

pub struct Candidates {
    bits: u16,
    i: u8,
}
impl Candidates {
    pub const fn new(bits: u16) -> Self {
        Self { bits: !bits, i: 0 }
    }
}
impl Iterator for Candidates {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        while self.bits != 0 && self.i < 9 {
            let current = self.bits;
            self.bits >>= 1;
            self.i += 1;

            if current & 1 == 1 {
                return Some(self.i);
            }
        }

        None
    }
}
