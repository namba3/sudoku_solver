# ソルバー実装と性能比較

## 既定バックエンド

| Target / 設定 | 公開solver APIの既定 | 選択理由 |
| --- | --- | --- |
| native x86_64 | SSE2 | 8セルの候補数を並列計算する。SSE2はx86_64のbaseline機能 |
| native x86 | SSE2。実行時非対応ならPacked-u64 | SSE2を実行時検出し、非対応CPUでは4セル処理へ切り替える |
| SIMD命令のないnative target | Packed-u64 | scalar版より速かった計測結果に基づく |
| WASM標準build | Packed-u64 | SIMD128を使わない標準設定 |
| WASM `wasm-simd` feature | SIMD128 | 対応engine向けの選択肢 |

`solver::backend::Optimized`はscalar実装、`solver::optimized_u64::Backend`は4個の`u16` maskを1つの`u64`に詰めるSWAR実装、`solver::optimized_simd::Backend`は8セルを処理するSIMD実装です。全バックエンドは同じ`SolverBackend`契約を使います。Minimality検証も`Optimized`、`OptimizedU64`、`OptimizedSimd`から選べます。

候補数が同じセルは盤面上で先に現れた方を選ぶため、これらのバックエンド間で解の列挙順と探索ノード数が一致します。

## ベンチマーク方法

```sh
cargo bench --locked --bench solver_backends
```

依存crateを追加せず、release build、`Instant`、`std::hint::black_box`で計測します。解探索は7サンプル各100回、Minimality検証は7サンプル各3回の中央値です。Minimal clue探索は18ヒントを3 seedで各1回調べます。探索ノード数は実装間で比較し、minimal-puzzle探索の成功数と実行時間は試行予算とともに読みます。確率的な探索の単一計測は性能保証ではありません。

## 解探索とMinimalityの実測

2026-10-30、x86_64 / AMD Ryzen 9 9900X、`rustc 1.101.0-nightly (76456a688 2026-10-25)`での一回のrelease計測です。各時間は7サンプルの中央値です。

| 処理 | 盤面 | Scalar Optimized | Packed-u64 | SSE2 | 探索ノード数 |
| --- | --- | ---: | ---: | ---: | ---: |
| 初回解まで | 30ヒント、easy-unique | 7.51 µs | 4.72 µs | 3.26 µs | 各52 |
| 最大2解を探索 | 30ヒント、easy-unique | 9.56 µs | 5.02 µs | 2.93 µs | 各52 |
| `solve` | 30ヒント、easy-unique | 9.84 µs | 2.98 µs | 2.74 µs | — |
| 初回解まで | 77ヒント、two-solution | 186 ns | 178 ns | 186 ns | 各5 |
| 最大2解を探索 | 77ヒント、two-solution | 235 ns | 241 ns | 327 ns | 各9 |
| `solve` | 77ヒント、two-solution | 146 ns | 143 ns | 147 ns | — |
| Minimality検証 | 17ヒントminimal | — | 238.26 ms | 233.81 ms | 各70,790 |
| Minimality検証 | 18ヒントnonminimal | — | 233.12 ms | 232.10 ms | 各62,135 |

Packed-u64は疎なeasy盤面でscalar版より速く、SSE2より遅い結果でした。77ヒント盤面は3方式とも数百nsで、差は小さく順位も処理ごとに変わります。SIMD非対応targetの既定をPacked-u64にしました。WASMとSIMD非対応CPUでの実行時間は未計測なので、target固有の性能は別途確認が必要です。

## Minimal clue探索

Naiveはランダム完成盤面からヒントを選び、Minimalityを確認します。OptimizedはDiverse探索で一意候補を作り、Minimalityを確認します。両方式は有限予算を共有しますが、探索空間が異なるため、探索ノード数だけでは比較できません。

2026-10-30の18ヒント探索は3 seed各1候補で、両方式ともMinimalを0件発見しました。Naiveは687.67 ms / 194 nodes、Optimizedは4.70 ms / 54,753 nodesで予算切れとなりました。発見がないため、この計測から成功速度の優劣は判断できません。各方式の詳細は[問題生成方式](generation.md)を参照してください。

## WASMのSIMD build

WASM標準buildはPacked-u64を使います。SIMD128版はCargo featureで明示的にbuildします。

```sh
cargo check --locked --target wasm32-unknown-unknown
cargo check --locked --tests --target wasm32-unknown-unknown --features wasm-simd
cargo build --locked --target wasm32-unknown-unknown --features wasm-simd --bin sudoku_solver
cargo build --locked --target wasm32-unknown-unknown --features "search-worker wasm-simd" --bin sudoku_search_worker
```

`wasm-simd`を有効にした成果物にはSIMD128命令が入り、対応engineが必要です。コンパイル確認は実行時性能の計測ではありません。
