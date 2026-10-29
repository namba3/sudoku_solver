#[path = "../cli/mod.rs"]
mod cli;
#[path = "../persistence.rs"]
mod persistence;

fn main() {
    if let Err(error) = cli::run(std::env::args().skip(1)) {
        eprintln!("sudoku: {error}");
        std::process::exit(2);
    }
}
