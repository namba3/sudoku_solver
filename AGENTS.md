# Repository guidance

## Project shape

- This is a Rust 2021 Sudoku solver with a Dioxus 0.7 WebAssembly UI.
- `src/lib.rs` exposes `Matrix`, `solve`, `candidates_for`, `find_hint`, and the unrestricted `SolutionSearch` iterator; keep the solver usable independently of the UI.
- `src/solver.rs` contains solving and enumeration implementations, result events, and unit tests.
- `src/main.rs` contains the browser UI and text conversion helpers.
- `src/bin/sudoku_search_worker.rs` is the separate WASM entry point for solution enumeration.
- The editable stylesheet is `public/app.css`.
- `documentation/` contains authored explanations. `docs/` contains generated GitHub Pages assets and is the published site root.

## Editing and documentation

- Write repository explanations in Japanese unless a specific artifact needs another language.
- Keep `README.md` as a short project overview and navigation page. Put detailed algorithm, architecture, usage, and development guidance in `documentation/`.
- Update `documentation/` when behavior, public APIs, formats, or build steps change.
- When rebuilding the Pages site, run `bash scripts/build_pages.sh`; preserve its root layout and the `/sudoku_solver/` base path. The script builds the worker and Dioxus bundle, then synchronizes only referenced generated assets into `docs/`.
- Do not hand-edit generated JavaScript or WebAssembly. Change Rust/CSS sources and regenerate the site assets when a published UI change is needed.

## Solver invariants

- A board is a 9×9 `Matrix`; values 1 through 9 are clues or placements and all other values are treated as empty by `solve`.
- Candidate masks use the lower nine bits, with bit `value - 1` representing each digit.
- Keep row, column, and 3×3 block masks synchronized when placing and undoing a guess.
- `solve` returns only whether it found a solution. It does not report why solving failed or whether the solution is unique; documentation must not imply otherwise.
- The solver's `SolutionSearch` iterator has no search limits. UI consumers may stop it at their own solution/node limits; a partial result must never be presented as an exact count.

## Verification

For Rust changes, run:

```sh
cargo fmt --check
cargo test
```

For Web UI or Dioxus changes, also run:

```sh
cargo check --target wasm32-unknown-unknown
```

Use a Dioxus CLI version compatible with the Dioxus dependency in `Cargo.toml`. A successful compile or bundle does not establish that the application was exercised in a browser.
