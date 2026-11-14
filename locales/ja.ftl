# Japanese translations.
language-switch = English
theme-label = ライトテーマ
theme-switch_light = ライトテーマに切り替え
theme-switch_dark = ダークテーマに切り替え
app-title = 数独ソルバー
help-input = 矢印キーで移動、数字キーで入力して次の空欄へ移動します。Ctrl/Cmd+Z: 元に戻す、Ctrl/Cmd+Y または Ctrl/Cmd+Shift+Z: やり直し。
help-given_toggle_shortcut = ロック解除中は、数字入りセルにフォーカスして G キーを押すと、初期数字／通常入力を切り替えられます。
help-naked_single_hint = 空セル自身の候補が1つだけのマス（Naked Single）を強調表示します。
help-hidden_single_hint = 行・列・3×3ブロック内で、ある数字を置ける空セルが1つだけのとき（Hidden Single）、そのセルを強調表示します。
help-single_hints_use_candidate_filters = ONでは候補フィルター適用後の候補からSingleを判定します。OFFではフィルター適用前の候補で判定します。
message-remaining_empty_cells = 残りの空セル: {$count}マス
help-candidate_display = 空セルに置ける候補数字を表示します。
help-locked_candidates = ブロック内の候補が同じ行・列に集まる場合や、行・列内の候補が同じブロックに集まる場合、その範囲外から同じ数字を除外します。
help-naked_subsets = 2〜4マスの候補数字の和集合がマス数と同じなら、その数字を同じ行・列・ブロック内のほかのマスから除外します。
help-hidden_subsets = 2〜4個の数字を置ける場所が同じ数のマスに限られるなら、そのマスからほかの候補数字を除外します。
help-fish = 2〜4行（列）の候補位置が同じ2〜4列（行）に収まる場合、その列（行）のほかのマスから数字を除外します（X-Wing / Swordfish / Jellyfish）。
help-xy_wing = 候補が2つの中心セルと、その一方を共有する2つの翼を探します。両翼の共通候補は、両方の翼を見るセルから除外できます。
help-skyscraper = ある数字の候補が2つだけの行（列）を2本探し、同じ列（行）にある候補をつなぎます。残る2候補を両方見通せるセルからその数字を除外します。
help-two_string_kite = ある数字の候補が2つだけの行と列を探します。同じブロックにある行側・列側の候補は同時に置けないため、残る2候補を両方見通せるセルからその数字を除外します。
help-w_wing = 同じ候補ペアを持つ2セルを、候補の一方の強リンクでつなぎます。強リンクの両端がそれぞれ片方のセルを見通すなら、2セルを両方見通せる場所からもう一方の候補を除外します。
help-candidate_filter_iterations = 各回では全フィルターが同じ候補状態を調べ、除外をまとめて反映します。候補が変わらなくても指定回数で終了します。
help-toggle_all_candidate_filters = スイッチで全フィルターを一括ON/OFFします。個別フィルターが1つ以上ONならON表示となり、押すとすべてOFFになります。
help-puzzle_generation_method = 既知の一意解初期盤面をランダムに選び、ランダムな対称変換を適用して生成します。
action-undo = 元に戻す
action-redo = やり直す
action-load_text = テキストから盤面へ読み込む
action-count_solutions = 解の数を調べる
action-generate = 問題を生成
action-cancel_generation = 生成を中断
action-cancel_search = 探索を中断
action-solve = 盤面を直接解く
action-reset = 初期盤面に戻す
action-clear = クリア
action-save_text = 盤面をテキストへ出力
action-show_candidates = 候補を表示
action-locked_candidates = Locked Candidates
action-naked_subsets = Naked Subsets（Pair / Triple / Quad）
action-hidden_subsets = Hidden Subsets（Pair / Triple / Quad）
action-toggle_all_candidate_filters = 候補フィルターを一括ON/OFF
action-fish = Fish（X-Wing / Swordfish / Jellyfish）
action-xy_wing = XY-Wing
action-skyscraper = Skyscraper
action-two_string_kite = 2-String Kite
action-w_wing = W-Wing
action-naked_single_hint = Naked Singleを強調
action-hidden_single_hint = Hidden Singleを強調
action-single_hints_use_candidate_filters = 候補フィルターを考慮
action-lock_givens = 初期数字をロック
action-unlock_givens = 初期数字を編集
action-previous_solution = 前の解
action-select_solution = 見つかった解を選択
action-next_solution = 次の解
action-apply_solution = 選択した解を適用
action-copy_text = テキストをコピー
group-board = 盤面操作
group-generation = 問題生成
group-transform = 対称変換
group-solve = 解く・確認
group-text = テキスト
label-board_panels = 盤面パネル
label-drag_bands = bandの入れ替え
label-drag_rows = 行の入れ替え
label-drag_stacks = stackの入れ替え
label-drag_columns = 列の入れ替え
label-clues = 初期数字数
label-candidate_display_section = 候補表示
label-candidate_filter_section = 候補フィルター
label-candidate_filter_iterations = 反復回数
label-single_hint_section = Singleの強調
label-hint_tab = ヒント
label-generation_tab = 問題生成
label-found_solutions = 見つかった解
label-puzzle_text = 問題のテキスト
label-bands = band（横3行）
label-stacks = stack（縦3列）
label-rows_in_band = band内の行
label-columns_in_stack = stack内の列
label-digits = 数字の置換
label-orientation = 向き・反転
label-first_band = 最初のband
label-second_band = 次のband
label-row_band = 対象band
label-first_row = 最初の行
label-second_row = 次の行
label-first_stack = 最初のstack
label-second_stack = 次のstack
label-column_stack = 対象stack
label-first_column = 最初の列
label-second_column = 次の列
label-first_digit = 数字A
label-second_digit = 数字B
help-puzzle_text = 9文字の行を9行入力してください。1〜9は初期数字、0・.・_は空欄です。
help-drag_board_transforms = band・stack・行・列の番号をドラッグすると、対応するまとまりを交換できます。行・列は同じband・stack内だけで交換します。番号を2つクリックまたはタップしても交換できます。
help-drag_preview_band = 交換プレビュー：band {$first} と band {$second} を入れ替えます。
help-drag_preview_stack = 交換プレビュー：stack {$first} と stack {$second} を入れ替えます。
help-drag_preview_row = 交換プレビュー：band {$band}内の{$first}行目と{$second}行目を入れ替えます。
help-drag_preview_column = 交換プレビュー：stack {$stack}内の{$first}列目と{$second}列目を入れ替えます。
help-drag_preview_invalid_row = 行の交換は同じband内だけで行えます。
help-drag_preview_invalid_column = 列の交換は同じstack内だけで行えます。
help-drag_preview_invalid_type = 同じ種類の番号同士をドラッグすると交換できます。
label-drag_band = band {$band}をドラッグまたは選択して交換
label-drag_row = band {$band}内の{$row}行をドラッグまたは選択して交換
label-drag_stack = stack {$stack}をドラッグまたは選択して交換
label-drag_column = stack {$stack}内の{$column}列をドラッグまたは選択して交換
message-invalid_clue_count = 初期数字数は17〜81の範囲で指定してください。
message-reset = 初期盤面に戻しました。
message-copied = コピーしました。
message-copy_failed = コピーできませんでした。
message-stale_solutions = これらの解は以前の盤面から得られたため、現在の盤面には適用できません。
message-puzzle_generated = 問題を生成しました。
message-transform_applied = 盤面を変換しました。
worker-error-unknown_generation = 問題生成ワーカーで不明なエラーが発生しました。
worker-error-invalid_board = 生成された盤面データが不正です。
worker-error-search_failed = 解の探索に失敗しました。
message-solution_range = {$total} 件中 {$selected} 件目
message-solution_preview = {$selected} 件目の解のプレビュー
message-cell_aria = {$row} 行 {$column} 列
message-conflicts = { $count ->
    *[other] 重複する数字があるセルは {$count} 個です。強調表示されたセルを修正してください。
    }
message-no_candidates = { $count ->
    *[other] 候補がない空欄が {$count} 個あります。強調表示されたセルを修正してください。
    }
message-puzzle_loaded = 問題を読み込みました。
message-puzzle_loaded_conflicts = { $count ->
    *[other] 問題を読み込みました。重複する数字があるセルは {$count} 個です。
    }
message-puzzle_loaded_no_candidates = { $count ->
    *[other] 問題を読み込みました。候補がない空欄が {$count} 個あります。
    }
search-starting = 解を探索しています…
search-generating = 一意解の問題を生成しています…
search-progress = { $count ->
    *[other] 探索中… {$nodes} ノードを調べ、{$count} 件の解が見つかりました。
    }
search-finished-solution_limit = { $count ->
    *[other] {$count} 件以上の解が見つかりました。{$nodes} ノード、{$seconds} 秒で解数上限に達しました。
    }
search-finished-node_limit = { $count ->
    *[other] {$nodes} ノード、{$seconds} 秒で探索を停止しました。{$count} 件の解が見つかりましたが、正確な解数は不明です。
    }
search-finished-complete = { $count ->
    *[other] 探索完了: {$nodes} ノードを{$seconds} 秒で調べ、解は {$count} 件です。
    }
search-cancelled = { $count ->
    *[other] 解を {$count} 件見つけた時点で探索を中断しました（{$seconds} 秒）。
    }
search-generated = { $count ->
    *[other] 一意解の問題を生成しました（初期数字 {$count} 個、{$seconds} 秒）。
    }
search-generation_cancelled = 問題の生成を中断しました（{$seconds} 秒）。
search-generation_error = 問題を生成できませんでした: {$message}（{$seconds} 秒）。
search-error = {$message}（{$seconds} 秒）
search-worker_error = 検索ワーカーでエラーが発生しました: {$message}
search-start_error = 解探索を開始できませんでした: {$message}
search-error-conflict = 同じ行・列・ブロックに重複する数字があります。
search-error-invalid_request = 盤面データまたは探索上限が不正です。
solve-success = 解けました（{$milliseconds} ミリ秒）。
solve-no_solution = 解が見つかりませんでした（{$milliseconds} ミリ秒）。
message-solution_applied = 選択した解を適用しました。
message-undo = 変更を元に戻しました。
message-redo = 変更をやり直しました。
action-swap_bands = bandを交換
action-swap_stacks = stackを交換
action-swap_rows = 行を交換
action-swap_columns = 列を交換
action-swap_digits = 数字を交換
action-transpose = 行列を転置
action-rotate_90 = 90°回転
action-rotate_180 = 180°回転
action-rotate_270 = 270°回転
action-reflect_horizontal = 上下反転
action-reflect_vertical = 左右反転
parse-line_count = 9行必要ですが、{$actual}行あります。
parse-line_length = {$row} 行目は9文字必要ですが、{$actual}文字あります。
parse-invalid_character = {$row} 行 {$column} 列に使えない文字があります。数字、0、.、_を使ってください。
