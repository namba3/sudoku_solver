# English translations.
language-switch = 日本語
theme-label = Light theme
theme-switch_light = Switch to light theme
theme-switch_dark = Switch to dark theme
app-title = Sudoku Solver
help-input = Use arrow keys to move. Enter a digit to jump to the next empty cell. Ctrl/Cmd+Z: Undo; Ctrl/Cmd+Y or Ctrl/Cmd+Shift+Z: Redo.
help-given_toggle_shortcut = While unlocked, focus a filled cell and press G to toggle it between an initial clue and a regular entry.
help-naked_single_hint = Highlight cells whose own candidate list contains exactly one digit (Naked Single).
help-hidden_single_hint = Highlight a cell when a digit has only one possible position in its row, column, or 3×3 box (Hidden Single).
help-single_hints_use_candidate_filters = When on, detect singles using candidates after the enabled filters. When off, use the original candidates before filtering.
message-remaining_empty_cells = Empty cells remaining: {$count}
help-candidate_display = Show the legal candidate digits in empty cells.
help-locked_candidates = When candidates for a digit are confined to one row or column within a box, or to one box within a row or column, remove that digit from the rest of the intersecting unit.
help-naked_subsets = If the union of candidates in 2–4 cells has the same size as the cell group, remove those digits from the other cells in that unit.
help-hidden_subsets = If 2–4 digits can appear only in the same number of cells, remove other candidates from those cells.
help-fish = If the candidate positions in 2–4 rows (or columns) are confined to the same 2–4 columns (or rows), remove that digit from the other cells in those cover lines (X-Wing, Swordfish, or Jellyfish).
help-xy_wing = Find a bivalue pivot and two wings that each share one pivot candidate. Remove the wings' shared candidate from cells that see both wings.
help-candidate_filter_iterations = Each iteration checks all enabled filters against the same candidate state and applies their eliminations together. Processing stops at the selected count, even if candidates stop changing sooner.
help-toggle_all_candidate_filters = Use this switch to turn every filter on or off. It shows on when any individual filter is on; activating it then turns all filters off.
help-puzzle_generation_method = A known uniquely solvable starting board is selected at random, then randomized symmetry transforms are applied.
action-undo = Undo
action-redo = Redo
action-load_text = Load text into board
action-count_solutions = Count solutions
action-generate = Generate puzzle
action-cancel_generation = Cancel generation
action-cancel_search = Cancel search
action-solve = Solve current board
action-reset = Restore initial board
action-clear = Clear
action-save_text = Save board to text
action-show_candidates = Show candidates
action-locked_candidates = Locked Candidates
action-naked_subsets = Apply Naked Subsets (pairs, triples, quads)
action-hidden_subsets = Apply Hidden Subsets (pairs, triples, quads)
action-toggle_all_candidate_filters = Toggle all candidate filters
action-fish = Apply Fish (X-Wing, Swordfish, Jellyfish)
action-xy_wing = Apply XY-Wing
action-naked_single_hint = Highlight Naked Singles
action-hidden_single_hint = Highlight Hidden Singles
action-single_hints_use_candidate_filters = Use candidate filters
action-lock_givens = Lock initial clues
action-unlock_givens = Edit initial clues
action-previous_solution = Previous solution
action-select_solution = Select found solution
action-next_solution = Next solution
action-apply_solution = Use selected solution
action-copy_text = Copy text
group-board = Board controls
group-generation = Puzzle generation
group-transform = Symmetry transforms
group-solve = Solve & inspect
group-text = Text
label-board_panels = Board panels
label-drag_bands = Reorder bands
label-drag_rows = Reorder rows
label-drag_stacks = Reorder stacks
label-drag_columns = Reorder columns
label-clues = Clues
label-candidate_display_section = Candidate display
label-candidate_filter_section = Candidate filters
label-candidate_filter_iterations = Iterations
label-single_hint_section = Single highlights
label-hint_tab = Hints
label-generation_tab = Generate
label-found_solutions = Found solutions
label-puzzle_text = Puzzle text
label-bands = Bands (three rows)
label-stacks = Stacks (three columns)
label-rows_in_band = Rows within a band
label-columns_in_stack = Columns within a stack
label-digits = Digit relabeling
label-orientation = Orientation and reflection
label-first_band = First band
label-second_band = Second band
label-row_band = Band
label-first_row = First row
label-second_row = Second row
label-first_stack = First stack
label-second_stack = Second stack
label-column_stack = Stack
label-first_column = First column
label-second_column = Second column
label-first_digit = Digit A
label-second_digit = Digit B
help-puzzle_text = Enter exactly 9 lines of 9 characters. Digits 1–9 are clues; 0, ., and _ are blank cells.
help-drag_board_transforms = Drag the band, stack, row, or column handles to swap them. Rows and columns can only be swapped within the same band or stack. You can also click or tap two handles to swap them.
help-drag_preview_band = Swap preview: Band {$first} and Band {$second} will be exchanged.
help-drag_preview_stack = Swap preview: Stack {$first} and Stack {$second} will be exchanged.
help-drag_preview_row = Swap preview: Row {$first} and row {$second} in Band {$band} will be exchanged.
help-drag_preview_column = Swap preview: Column {$first} and column {$second} in Stack {$stack} will be exchanged.
help-drag_preview_invalid_row = Rows can only be swapped within the same band.
help-drag_preview_invalid_column = Columns can only be swapped within the same stack.
help-drag_preview_invalid_type = Drag handles of the same type to swap them.
label-drag_band = Drag or select band {$band} to swap
label-drag_row = Drag or select row {$row} in band {$band} to swap
label-drag_stack = Drag or select stack {$stack} to swap
label-drag_column = Drag or select column {$column} in stack {$stack} to swap
message-invalid_clue_count = Enter a clue count between 17 and 81.
message-reset = Restored the initial board.
message-copied = Copied to clipboard.
message-copy_failed = Could not copy the text.
message-stale_solutions = These solutions are from an earlier board and cannot be applied to the current board.
message-puzzle_generated = Puzzle generated.
message-transform_applied = Board transformed.
worker-error-unknown_generation = The puzzle generation worker returned an unknown error.
worker-error-invalid_board = The generated board data is invalid.
worker-error-search_failed = Solution search failed.
message-solution_range = Solution {$selected} of {$total}
message-solution_preview = Preview of solution {$selected}
message-cell_aria = Row {$row}, column {$column}
message-conflicts = { $count ->
    [one] Resolve the conflict in {$count} highlighted cell.
    *[other] Resolve conflicts in {$count} highlighted cells.
    }
message-no_candidates = { $count ->
    [one] No candidates remain in {$count} empty cell.
    *[other] No candidates remain in {$count} empty cells.
    }
message-puzzle_loaded = Puzzle loaded.
message-puzzle_loaded_conflicts = { $count ->
    [one] Puzzle loaded. Resolve the conflict in {$count} highlighted cell.
    *[other] Puzzle loaded. Resolve conflicts in {$count} highlighted cells.
    }
message-puzzle_loaded_no_candidates = { $count ->
    [one] Puzzle loaded. No candidates remain in {$count} empty cell.
    *[other] Puzzle loaded. No candidates remain in {$count} empty cells.
    }
search-starting = Starting solution search…
search-generating = Generating a unique puzzle…
search-progress = { $count ->
    [one] Searching… {$count} solution found across {$nodes} nodes.
    *[other] Searching… {$count} solutions found across {$nodes} nodes.
    }
search-finished-solution_limit = { $count ->
    [one] At least {$count} solution found; the solution limit was reached after {$nodes} nodes in {$seconds} s.
    *[other] At least {$count} solutions found; the solution limit was reached after {$nodes} nodes in {$seconds} s.
    }
search-finished-node_limit = { $count ->
    [one] Search stopped at {$nodes} nodes after finding {$count} solution in {$seconds} s. The exact count is unknown.
    *[other] Search stopped at {$nodes} nodes after finding {$count} solutions in {$seconds} s. The exact count is unknown.
    }
search-finished-complete = { $count ->
    [one] Search complete: exactly {$count} solution found across {$nodes} nodes in {$seconds} s.
    *[other] Search complete: exactly {$count} solutions found across {$nodes} nodes in {$seconds} s.
    }
search-cancelled = { $count ->
    [one] Search cancelled after finding {$count} solution in {$seconds} s.
    *[other] Search cancelled after finding {$count} solutions in {$seconds} s.
    }
search-generated = { $count ->
    [one] Generated a unique puzzle with {$count} clue in {$seconds} s.
    *[other] Generated a unique puzzle with {$count} clues in {$seconds} s.
    }
search-generation_cancelled = Puzzle generation cancelled after {$seconds} s.
search-generation_error = Puzzle generation failed: {$message} ({$seconds} s).
search-error = {$message} ({$seconds} s)
search-worker_error = Solution worker failed: {$message}
search-start_error = Could not start solution search: {$message}
search-error-conflict = Conflicting values.
search-error-invalid_request = Invalid board data or search limits.
solve-success = Solved in {$milliseconds} ms!
solve-no_solution = No solution found, {$milliseconds} ms.
message-solution_applied = Selected solution applied.
message-undo = Change undone.
message-redo = Change redone.
action-swap_bands = Swap bands
action-swap_stacks = Swap stacks
action-swap_rows = Swap rows
action-swap_columns = Swap columns
action-swap_digits = Swap digits
action-transpose = Transpose
action-rotate_90 = Rotate 90°
action-rotate_180 = Rotate 180°
action-rotate_270 = Rotate 270°
action-reflect_horizontal = Flip vertically
action-reflect_vertical = Flip horizontally
parse-line_count = Expected 9 lines, found {$actual}.
parse-line_length = Line {$row} must contain 9 characters, found {$actual}.
parse-invalid_character = Unsupported character at row {$row}, column {$column}. Use digits, 0, . or _.
