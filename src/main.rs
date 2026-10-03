use dioxus::prelude::*;
use wasm_bindgen::JsCast;

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();
    dioxus::launch(app);
}

const INITIAL_MTX: sudoku_solver::Matrix = [
    [0, 8, 0, 0, 0, 0, 1, 5, 0],
    [4, 0, 6, 5, 0, 9, 0, 8, 0],
    [0, 0, 0, 0, 0, 8, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 2, 0, 4, 0, 0, 0, 3],
    [3, 0, 0, 8, 0, 1, 0, 0, 0],
    [9, 0, 0, 0, 7, 0, 0, 0, 0],
    [6, 0, 0, 0, 0, 0, 0, 0, 4],
    [1, 5, 0, 0, 0, 0, 0, 9, 0],
];

fn app() -> Element {
    let mut mtx = use_signal(|| INITIAL_MTX);
    let mut givens = use_signal(|| INITIAL_MTX);
    let mut txt = use_signal(|| to_txt(&INITIAL_MTX));
    let mut msg = use_signal(String::new);
    let mut is_ok = use_signal(|| true);

    let msg_class = if is_ok() { "msg ok" } else { "msg error" };
    let conflicts = conflicting_cells(&mtx.read());

    rsx! {
        div {
            class: "container",
            h1 { "Sudoku Solver" }
            p { class: "{msg_class}", role: "status", " {msg} " }
            p {
                class: "input-help",
                "Use the arrow keys to move. Enter a digit to jump to the next empty cell."
            }
            ul {
                class: "matrix",
                for (y, row) in mtx.read().iter().enumerate() {
                    li {
                        key: "{y}",
                        ul {
                            class: "row",
                            for (x, cell) in row.iter().copied().enumerate() {
                                li {
                                    class: "{cell_classes(y, x, givens.read()[y][x] != 0, conflicts[y][x])}",
                                    key: "{y}-{x}",
                                    input {
                                        id: "cell-{y}-{x}",
                                        r#type: "number",
                                        max: "9",
                                        min: "1",
                                        inputmode: "numeric",
                                        value: "{cell_text(cell)}",
                                        aria_label: "Row {y + 1}, column {x + 1}",
                                        aria_invalid: "{conflicts[y][x]}",
                                        oninput: move |evt| {
                                            let mut board = *mtx.read();
                                            board[y][x] = cell_value(&evt.value());
                                            mtx.set(board);

                                            let mut original_clues = *givens.read();
                                            original_clues[y][x] = 0;
                                            givens.set(original_clues);

                                            let conflict_count = count_conflict_cells(&board);
                                            if conflict_count > 0 {
                                                msg.set(format!("Resolve conflicts in {conflict_count} highlighted cells."));
                                                is_ok.set(false);
                                            } else {
                                                msg.set(String::new());
                                                is_ok.set(true);
                                            }

                                            if board[y][x] != 0 {
                                                if let Some((next_y, next_x)) = next_empty_cell(&board, (y, x)) {
                                                    focus_cell(next_y, next_x);
                                                }
                                            }
                                        },
                                        onkeydown: move |evt| {
                                            let key = evt.key().to_string();
                                            let movement = match key.as_str() {
                                                "ArrowUp" => Some((-1, 0)),
                                                "ArrowDown" => Some((1, 0)),
                                                "ArrowLeft" => Some((0, -1)),
                                                "ArrowRight" => Some((0, 1)),
                                                _ => None,
                                            };

                                            if let Some((dy, dx)) = movement {
                                                evt.prevent_default();
                                                if let Some((next_y, next_x)) = moved_cell(y, x, dy, dx) {
                                                    focus_cell(next_y, next_x);
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div {
                class: "buttons",
                button {
                    class: "left",
                    onclick: move |_| {
                        match parse_puzzle(&txt.read()) {
                            Ok(board) => {
                                mtx.set(board);
                                givens.set(board);
                                let conflict_count = count_conflict_cells(&board);
                                if conflict_count > 0 {
                                    msg.set(format!("Puzzle loaded. Resolve conflicts in {conflict_count} highlighted cells."));
                                    is_ok.set(false);
                                } else {
                                    msg.set("Puzzle loaded.".to_string());
                                    is_ok.set(true);
                                }
                                focus_cell(0, 0);
                            }
                            Err(error) => {
                                msg.set(error);
                                is_ok.set(false);
                            }
                        }
                    },
                    "↑ Load text"
                }
                button {
                    onclick: move |_| {
                        let mut solver_mtx = *mtx.read();
                        let conflict_count = count_conflict_cells(&solver_mtx);

                        if conflict_count > 0 {
                            msg.set(format!("Resolve conflicts in {conflict_count} highlighted cells before solving."));
                            is_ok.set(false);
                        } else {
                            let window = web_sys::window().unwrap();
                            let performance = window.performance().unwrap();
                            let t_start = performance.now();
                            let succeeded = sudoku_solver::solve(&mut solver_mtx);
                            let ms = performance.now() - t_start;
                            log::debug!("time: {ms:.0} ms");

                            if succeeded {
                                mtx.set(solver_mtx);
                                msg.set(format!("Solved in {ms:.0} ms!"));
                                is_ok.set(true);
                            } else {
                                msg.set(format!("No solution found, {ms:.0} ms."));
                                is_ok.set(false);
                            }
                        }
                    },
                    "Solve"
                }
                button {
                    onclick: move |_| {
                        mtx.set([[0; 9]; 9]);
                        givens.set([[0; 9]; 9]);
                        msg.set(String::new());
                        is_ok.set(true);
                        focus_cell(0, 0);
                    },
                    "Clear"
                }
                button {
                    class: "right",
                    onclick: move |_| {
                        txt.set(to_txt(&mtx.read()));
                    },
                    "Save text ↓"
                }
            }
            div {
                class: "text-panel",
                label {
                    r#for: "puzzle-text",
                    "Puzzle text"
                }
                p {
                    class: "text-help",
                    "Enter exactly 9 lines of 9 characters. Digits 1–9 are clues; 0, ., and _ are blank cells."
                }
                textarea {
                    id: "puzzle-text",
                    class: "text",
                    value: "{txt}",
                    onchange: move |evt| {
                        txt.set(evt.value());
                    }
                }
            }
        }
    }
}

fn cell_value(s: &str) -> u8 {
    s.parse::<u8>()
        .ok()
        .filter(|value| (1..=9).contains(value))
        .unwrap_or(0)
}

fn cell_text(cell: u8) -> String {
    if (1..=9).contains(&cell) {
        cell.to_string()
    } else {
        String::new()
    }
}

fn cell_classes(y: usize, x: usize, is_given: bool, is_conflict: bool) -> String {
    let shade = if ((y / 3) + (x / 3)) % 2 == 1 {
        "odd"
    } else {
        "even"
    };
    let given = if is_given { " given" } else { "" };
    let conflict = if is_conflict { " conflict" } else { "" };
    format!("cell {shade}{given}{conflict}")
}

fn parse_puzzle(txt: &str) -> Result<sudoku_solver::Matrix, String> {
    let mut mtx = sudoku_solver::Matrix::default();
    let content = txt.trim_end_matches(|ch| ch == '\n' || ch == '\r');
    let lines: Vec<_> = content.lines().collect();
    if lines.len() != 9 {
        return Err(format!("Expected 9 lines, found {}.", lines.len()));
    }

    for (y, line) in lines.iter().enumerate() {
        let chars: Vec<_> = line.chars().collect();
        if chars.len() != 9 {
            return Err(format!(
                "Line {} must contain 9 characters, found {}.",
                y + 1,
                chars.len()
            ));
        }
        for (x, ch) in chars.into_iter().enumerate() {
            mtx[y][x] = match ch {
                '1'..='9' => ch as u8 - b'0',
                '0' | '.' | '_' => 0,
                _ => {
                    return Err(format!(
                        "Unsupported character at row {}, column {}. Use digits, 0, . or _.",
                        y + 1,
                        x + 1
                    ));
                }
            };
        }
    }
    Ok(mtx)
}

fn to_txt(mtx: &sudoku_solver::Matrix) -> String {
    let mut s = String::with_capacity((9 + 1) * 9);
    for row in mtx {
        for &cell in row {
            s.push(match cell {
                1..=9 => char::from(b'0' + cell),
                _ => '_',
            });
        }
        s.push('\n');
    }
    s
}

fn conflicting_cells(board: &sudoku_solver::Matrix) -> [[bool; 9]; 9] {
    let mut conflicts = [[false; 9]; 9];
    for y in 0..9 {
        for x in 0..9 {
            let value = board[y][x];
            if !(1..=9).contains(&value) {
                continue;
            }
            for other_y in 0..9 {
                for other_x in 0..9 {
                    if (other_y, other_x) <= (y, x) {
                        continue;
                    }
                    let shares_unit = y == other_y
                        || x == other_x
                        || (y / 3 == other_y / 3 && x / 3 == other_x / 3);
                    if shares_unit && board[other_y][other_x] == value {
                        conflicts[y][x] = true;
                        conflicts[other_y][other_x] = true;
                    }
                }
            }
        }
    }
    conflicts
}

fn count_conflict_cells(board: &sudoku_solver::Matrix) -> usize {
    conflicting_cells(board)
        .iter()
        .flatten()
        .filter(|&&has_conflict| has_conflict)
        .count()
}

fn moved_cell(y: usize, x: usize, dy: isize, dx: isize) -> Option<(usize, usize)> {
    let next_y = y as isize + dy;
    let next_x = x as isize + dx;
    if (0..9).contains(&next_y) && (0..9).contains(&next_x) {
        Some((next_y as usize, next_x as usize))
    } else {
        None
    }
}

fn next_empty_cell(
    board: &sudoku_solver::Matrix,
    (y, x): (usize, usize),
) -> Option<(usize, usize)> {
    let current = y * 9 + x;
    (1..81)
        .map(|offset| (current + offset) % 81)
        .map(|index| (index / 9, index % 9))
        .find(|&(next_y, next_x)| board[next_y][next_x] == 0)
}

fn focus_cell(y: usize, x: usize) {
    let id = format!("cell-{y}-{x}");
    let Some(input) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(&id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
    else {
        return;
    };
    let _ = input.focus();
}
