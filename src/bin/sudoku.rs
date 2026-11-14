#[cfg(not(target_arch = "wasm32"))]
#[path = "../cli/mod.rs"]
mod cli;
#[cfg(not(target_arch = "wasm32"))]
#[path = "../persistence.rs"]
mod persistence;
#[cfg(not(target_arch = "wasm32"))]
#[path = "../puzzle_text.rs"]
mod puzzle_text;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = cli::run(std::env::args().skip(1)) {
        eprintln!("sudoku: {error}");
        std::process::exit(2);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}
