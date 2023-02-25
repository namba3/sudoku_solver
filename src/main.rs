use dioxus::prelude::*;
fn main() {
    // init debug tool for WebAssembly
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();

    dioxus_web::launch(app);
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

fn app(cx: Scope) -> Element {
    let mtx: Vec<Vec<_>> = (0..9)
        .into_iter()
        .map(|y| {
            (0..9)
                .into_iter()
                .map(|x| use_state(&cx, || INITIAL_MTX[y][x]))
                .collect()
        })
        .collect();
    let mtx = std::rc::Rc::new(mtx);
    let txt = use_state(&cx, || to_txt(&mtx));
    let msg = use_state(&cx, || String::new());
    let is_ok = use_state(&cx, || true);

    let msg_class = if **is_ok { "msg ok" } else { "msg error" };

    cx.render(rsx! {
        div {
            class: "container",
            h1 { "Sudoku Solver" }
            p { class: "{msg_class}", " {msg} " }
            ul { class: "matrix", mtx.iter().enumerate().map(|(y, row)| rsx! {
                li {
                    key: "{y}",
                    ul { class: "row", row.iter().copied().enumerate().map(|(x, c)| {
                        let val = if (1..=9u8).contains(c) { format!("{c}") } else { String::from(" ") };
                        let odd_even = if ((y/3)+x/3) % 2 == 1 {
                            "odd"
                        } else {
                            "even"
                        };

                        rsx! {
                            li {
                                class: "cell {odd_even}",
                                key: "{y}-{x}",
                                input {
                                    r#type: "number",
                                    max: "9",
                                    min: "1",
                                    value: "{val}",
                                    oninput: move |evt| {
                                        let val = cell_value(&evt.value);
                                        c.set(val);
                                        msg.set(String::new());
                                        is_ok.set(true);
                                    },
                                }
                            }
                        }
                    })}
                }
            })}
            div {
                class: "buttons",
                button {
                    class: "allow-up left",
                    onclick: {
                        let mtx = mtx.clone();
                        move |_evt| {
                            from_txt(&mtx, txt);
                            msg.set(String::new());
                            is_ok.set(true);
                        }
                    },
                    "↑"
                }
                button {
                    onclick: {
                        let mtx = mtx.clone();
                        move |_evt| {
                            let mut solver_mtx = translate(&mtx);

                            let window = web_sys::window().unwrap();
                            let performance = window.performance().unwrap();
                            let t_start = performance.now();
                            let succeeded = sudoku_solver::solve(&mut solver_mtx);
                            let ms = performance.now() - t_start;
                            log::debug!("time: {ms:.0} ms");

                            if  succeeded {
                                write_back(&mtx, &solver_mtx);
                                msg.set(format!("Solved in {ms:.0} ms!"));
                                is_ok.set(true);
                            }
                            else {
                                msg.set(format!("Failed to solve, {ms:.0} ms"));
                                is_ok.set(false);
                            }
                        }
                    },
                    "Solve"
                }
                button {
                    onclick: {
                        let mtx = mtx.clone();
                        move |_evt| {
                            clear(&mtx);
                            msg.set(String::new());
                            is_ok.set(true);
                        }
                    },
                    "Clear"
                }
                button {
                    class: "arrow-down right",
                    onclick: {
                        let mtx = mtx.clone();
                        move |_evt| {
                            txt.set(to_txt(&mtx));
                        }
                    },
                    "↓"
                }
            }
            div {
                textarea {
                    class: "text",
                    value: "{txt}",
                    onchange: move |evt| {
                        txt.set(evt.value.clone());
                    }
                }
            }
        }
    })
}

fn cell_value(s: &str) -> u8 {
    let val = match s.as_bytes().get(0) {
        Some(ch) if ch.is_ascii_digit() => *ch - b'0',
        _ => 0,
    };
    match s.as_bytes().get(1) {
        Some(ch) if ch.is_ascii_digit() => *ch - b'0',
        Some(b' ') => 0,
        _ => val,
    }
}

fn translate(ui_mtx: &Vec<Vec<&UseState<u8>>>) -> sudoku_solver::Matrix {
    let mut mtx = sudoku_solver::Matrix::default();
    for y in 0..9 {
        for x in 0..9 {
            mtx[y][x] = **ui_mtx[y][x];
        }
    }
    mtx
}

fn write_back(ui_mtx: &Vec<Vec<&UseState<u8>>>, solver_mtx: &sudoku_solver::Matrix) {
    for y in 0..9 {
        for x in 0..9 {
            ui_mtx[y][x].set(solver_mtx[y][x]);
        }
    }
}

fn clear(ui_mtx: &Vec<Vec<&UseState<u8>>>) {
    for row in ui_mtx.iter() {
        for cell in row.iter() {
            cell.set(0);
        }
    }
}

fn from_txt(ui_mtx: &Vec<Vec<&UseState<u8>>>, txt: &str) {
    if txt.is_empty() {
        return;
    }

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
    write_back(ui_mtx, &mtx);
}

fn to_txt(ui_mtx: &Vec<Vec<&UseState<u8>>>) -> String {
    let mut s = String::with_capacity((9 + 1) * 9);
    for row in ui_mtx.iter() {
        for &cell in row.iter() {
            let ch = match **cell {
                1..=9 => **cell + b'0',
                _ => b'_',
            };
            s.push(char::from_u32(ch as u32).unwrap());
        }
        s.push('\n');
    }
    s
}
