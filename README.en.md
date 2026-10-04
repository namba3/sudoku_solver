# Sudoku Solver

English | [日本語](README.md)

A Rust Sudoku solver with a browser UI built using Dioxus. It supports entering and solving puzzles, generating puzzles with a unique solution, displaying candidates and solution counts, loading puzzles from text, and highlighting duplicate digits or empty cells with no candidates. The solver also provides a one-move hint API; the hint control is temporarily hidden in the browser UI.

Live app: <https://namba3.github.io/sudoku_solver/>

## Documentation

Detailed documentation is maintained in Japanese under [`documentation/`](documentation/README.md):

- [Algorithm](documentation/algorithm.md): candidate masks, MRV, and backtracking
- [Application and code structure](documentation/architecture.md)
- [Usage and puzzle format](documentation/usage.md)
- [Development and build](documentation/development.md)

## Get started

Install Rust and a Dioxus CLI version in the 0.7 series to match this project's Dioxus dependencies.

```sh
cargo test
dx serve --platform web
```

Generated GitHub Pages assets go in `docs/`. Edit explanatory documentation in `documentation/` instead.
