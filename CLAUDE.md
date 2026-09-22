# jind — 引き継ぎ(2026-09-22)

jev × find。単語の並びを find コマンドにして、確認してから実行する CLI。jurl(jev × curl、`../jurl`)の姉妹プロジェクトで、骨組み・jev クライアント・確認 UI は jurl から流用している。

## いまの状態

- GitHub `yukihirop/jind`(public)を 2026-09-22 に作成、`main` を push 済み。**crates.io は未 publish**
  - publish は README を確認してから、とユーザーが言っている。**publish 前に必ず確認を取る**
- `cargo install --path .` 済み(`~/.cargo/bin/jind` 0.1.0)
- テスト 17 件 pass、clippy 0 件、edition 2024、rustc 1.98
- crates.io の名前 `jind` は空いていた(2026-09-22 に API で 404 を確認)

## 構成(`src/`)

`rules::classify` → 未解決があれば `interpret::interpret`(= `jev::prompt::build` → `Oracle::decide` → `jev::prompt::apply` → `repair::repair`)→ `assemble::assemble` → `find::argv` → 確認 → `find::run_*`

| ファイル | 役割 |
|---|---|
| `token.rs` | `Role`(path / name_pattern / extension / name_word / type / time_amount / size_amount / depth / unit / qualifier / exclude_marker / exclude / depth_marker / empty / action / find_arg / noise)、`Unit`、`Amount { n, unit, at_least }` |
| `rules.rs` | 規則分類。パス(実在ディレクトリも)、glob、type 語、action 語、`+7d` `>10M`(符号と単位が揃ったときだけ確定)、単位語(数の直後なら Unit、無ければ「1 つ」の量)、`today`、`except`、`depth`、`-perm 644` 素通し |
| `jev/prompt.rs` | 質問: `role.i` Choice、数には `unit.i` Choice(次が単位語なら聞かない)と `atleast.i` Noul(以上か以下か)、語には `type_typo.i` / `action_typo.i`。規則で量と決まって向きだけ無いもの(`a week`)にも `atleast.i` |
| `repair.rs` | `attach_units`(単位が jev の役割より強い)、`mark_depth`、`mark_excludes`(except の後ろ、Noise は跨ぐ) |
| `assemble.rs` | `Search`。時間は日なら `-mtime`、時/分なら `-mmin`、週は日に。`-size` は整数なので `1.5GB` → `+1536M` |
| `find.rs` | argv 生成(`-maxdepth` を先頭に、複数 name は `\( -o \)`、除外は `-not -path '*/X/*'` と `'*/X'`)、1 行表示、`run_inherit` / `run_capture` |
| `demo.rs` | `jind demo`。見本の木 `TREE` / `EMPTY_DIRS` と ↑↓ picker(raw termios、unix のみ) |
| `main.rs` | `run`(サブコマンド振り分け)→ `execute`(本体)。確認フロー。**delete は `-y` でも必ず確認、既定 No、`-delete` 抜きで先に回して最大 10 件見せる**。`count` は行数を数えて表示 |
| `config.rs` | `~/.config/jind/config.toml`。鍵が無ければ `~/.config/jurl/config.toml` の `[jev] api_key` を借りる |
| `interpret.rs` | mock Oracle の統合テスト 5 件(聞いていないキーに答えると panic する Mock) |

jurl から `color.rs` `jev/{mod,client}.rs` `setup.rs` をほぼそのままコピーしている。jurl 側を直したらこちらも見る。

## 実機で確認した挙動(typesafe/jev-1.13-20260917)

- `log files older than 7 days in /var/log delete` → `find /var/log -type f -iname '*.log' -mtime +7 -delete`(5 問、~300–700 ms、$0.00007)
- `rs edited within an hour` → `-mmin -60`、`log files older than a week` → `-mtime +7`
- `png bigger than 5MB in ~/Downloads` → `-iname '*.png' -size +5M`
- `big mp4 over 1.5GB ls` → `-size +1536M -ls`
- pty で delete の確認フローを通し、1 件だけ消えることを確認済み

## 弱いところ・未着手

- 裸の語が extension か name_word かで jev が 0.6〜0.9 に割れる(`log` 0.63〜0.71)。確認プロンプトは出る。`*.log` と書けば規則で即実行
- `png images …` の `images` のような分類語は name_word 0.46 で拒否になる
- `jind demo [N]`(`demo.rs`、2026-09-22): jurl の picker を流用。`$TMPDIR/jind-demo` に見本の木(mtime を日数で戻す、sparse で 6MB/12MB)を毎回作り直して chdir。例 10 本は実機で通した。`rs` 単独や `tmp`(空ディレクトリ `tmp/` を作ると規則で path になる)、typo `flies` は jev が割れて demo に載せられなかった
- 時間は mtime のみ。`yesterday`(ちょうど 1 日)や `-newer file` は無い
- README は jurl と同じ構成(`docs/hero.svg` `flow.svg` `demo.svg`)。demo は `docs/demo-capture.json`(pty で取った実出力)→ `docs/make-demo.py`。例は `-n` で実出力と一致することを確認済み(2026-09-22)
- Windows は未検証(jurl と同様)

## 作業ルール(ユーザーから)

- 日本語で答える。事実と推測を分ける
- 小さな修正は commit → push まで確認なしでよい(jurl で 2026-09-22 に了承。jind も同じ扱い)
- 公開・publish・削除は必ず事前に確認
- jev の API / モデル ID の一次資料は `~/JavaScriptProjects/eg-jev`(`packages/recipes/src/lib/{openrouter,questions}.ts`)
- commit 末尾に `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`

## 動作確認のコツ

- zsh で `$c` を展開すると 1 語になる。複数語を渡すときは関数で `"$@"`(`/tmp/jt.sh` の形)
- この環境の `find` はシェル関数(bfs のシム)。`Command::new("find")` は `/usr/bin/find`(BSD)を実行するので影響なし
- 対話フローは Python の `pty.fork()` で試せる(`[y/N/e]` を待って書く)
