use dioxus::prelude::*;

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
    let mut txt = use_signal(|| to_txt(&INITIAL_MTX));
    let mut msg = use_signal(String::new);
    let mut is_ok = use_signal(|| true);

    let msg_class = if is_ok() { "msg ok" } else { "msg error" };

    rsx! {
        div {
            class: "container",
            h1 { "Sudoku Solver" }
            p { class: "{msg_class}", " {msg} " }
            ul {
                class: "matrix",
                for (y, row) in mtx.read().iter().enumerate() {
                    li {
                        key: "{y}",
                        ul {
                            class: "row",
                            for (x, cell) in row.iter().copied().enumerate() {
                                li {
                                    class: "cell {cell_class(y, x)}",
                                    key: "{y}-{x}",
                                    input {
                                        r#type: "number",
                                        max: "9",
                                        min: "1",
                                        value: "{cell_text(cell)}",
                                        oninput: move |evt| {
                                            mtx.write()[y][x] = cell_value(&evt.value());
                                            msg.set(String::new());
                                            is_ok.set(true);
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
                        mtx.set(from_txt(&txt.read()));
                        msg.set(String::new());
                        is_ok.set(true);
                    },
                    "↑ Load text"
                }
                button {
                    onclick: move |_| {
                        let mut solver_mtx = *mtx.read();

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
                            msg.set(format!("Failed to solve, {ms:.0} ms"));
                            is_ok.set(false);
                        }
                    },
                    "Solve"
                }
                button {
                    onclick: move |_| {
                        mtx.set([[0; 9]; 9]);
                        msg.set(String::new());
                        is_ok.set(true);
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
                    "Use 9 lines of 9 characters. Digits are clues; other characters are blank cells."
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
    let val = match s.as_bytes().first() {
        Some(ch) if ch.is_ascii_digit() => *ch - b'0',
        _ => 0,
    };
    match s.as_bytes().get(1) {
        Some(ch) if ch.is_ascii_digit() => *ch - b'0',
        Some(b' ') => 0,
        _ => val,
    }
}

fn cell_text(cell: u8) -> String {
    if (1..=9).contains(&cell) {
        cell.to_string()
    } else {
        String::new()
    }
}

fn cell_class(y: usize, x: usize) -> &'static str {
    if ((y / 3) + (x / 3)) % 2 == 1 {
        "odd"
    } else {
        "even"
    }
}

fn from_txt(txt: &str) -> sudoku_solver::Matrix {
    let mut mtx = sudoku_solver::Matrix::default();
    for (y, line) in txt.lines().take(9).enumerate() {
        for (x, ch) in line.chars().take(9).enumerate() {
            mtx[y][x] = if ch.is_ascii_digit() {
                ch as u8 - b'0'
            } else {
                0
            };
        }
    }
    mtx
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
