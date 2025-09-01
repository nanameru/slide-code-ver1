# リポジトリガイドライン

## プロジェクト構成とモジュール
- 目的: ratatui による端末 UI（チャット/プレビュー/ポップアップ）。
- クレート: `slide-tui`（CLI から利用）。
- 主なファイル:
  - `src/app.rs`（メイン UI ループ、ポップアップ、ステータスバー）
  - `src/agent.rs`（`slide-core` Codex とのブリッジ）
  - `src/preview.rs`、`src/interactive.rs`、`src/widgets/*`

## ビルド/テスト/開発コマンド
- 実行: `cd slide-rs/tui && cargo run`
- ビルド: `cd slide-rs && cargo build -p slide-tui`
- テスト: `cd slide-rs && cargo test -p slide-tui`
- ランチャー経由: `npm run dev` または `./slide.sh`

## コーディング規約
- Rust 2021・イベント駆動。描画コードは純粋関数的に保ち、ブロッキング処理を避ける。
- ウィジェットは `widgets/` に分離。ビジネスロジックを描画パスへ持ち込まない。
- 小さな合成可能なウィジェットを尊重し、不変の入力でレンダリング。

## テスト方針
- 可能な範囲でウィジェット出力のスナップショット/文字列アサーションを活用。
- 決定的（幅固定・タイマ不使用）に保つ。
- エージェントイベントはフェイクを用い、決定的なデルタをチャットに供給。

## 備考
- core のイベント（AgentMessageDelta/TaskStarted など）を消費して描画。
- キーバインド: `i` 挿入、`h` ヘルプ、`/` 検索、`:` パレット、`q` 終了。
- ポップアップ: ファイル検索/コマンドパレット、`slides/` の MRU 対応。
- プレビュー: ←/→（j/k）、Home/End、`h` でヘルプ。
