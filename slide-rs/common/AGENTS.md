# リポジトリガイドライン

## プロジェクト構成とモジュール
- 目的: 共有型/ユーティリティ（設定、承認モード、ファイルユーティリティ）。
- クレート: `slide-common`（ライブラリ）。
- 主なファイル:
  - `src/config.rs`, `src/approval_mode.rs`, `src/file_utils.rs`, `src/lib.rs`。

## ビルド/テスト/開発コマンド
- ビルド: `cd slide-rs && cargo build -p slide-common`
- テスト: `cd slide-rs && cargo test -p slide-common`

## コーディング規約
- Rust 2021。ヘルパは純粋に、getter に副作用を持たない。
- 設定のシリアライズは `serde`、ファイル I/O は `tokio` 非同期で。
- 列挙は `#[serde(rename_all = "kebab-case")]` を基本にし互換性を維持。

## テスト方針
- 設定の load/save/デフォルト、ファイルユーティリティ（スラッグ/ファイル名）をユニットテスト。
- 承認モードのパース、パス正規化、エラーメッセージを検証。

## 備考
- CLI/TUI/Core から利用されるため依存は軽量に。
- 設定パス解決はクロスプラットフォーム（`dirs`）、ファイルパーミッションは範囲外。
