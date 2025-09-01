# Repository Guidelines

## 編集後
編集したファイルをすべてコミットアンドプッシュします。

## プロジェクト構成とモジュール
- `slide-rs/`（Rust ワークスペース）:
  - `cli/`（CLI エントリ）、`tui/`（端末 UI）、`core/`（エージェント中核：セッション/ツール/安全性）、`common/`（設定/ユーティリティ）、`chatgpt/`（HTTP クライアント）ほか。
- `slide-cli/`（Node ランチャー）: ビルド済みバイナリの起動。`bin/slide.js` 参照。
- `slides/`（生成スライド）、`docs/`（設計メモ）、`slide.sh`（ローカル起動）。

## ビルド/テスト/開発コマンド
- 開発実行: `npm run dev` または `./slide.sh`（Rust ビルド→CLI 起動）。
- リリースビルド: `npm run build` または `cd slide-rs && cargo build --release`。
- テスト: `npm run test` または `cd slide-rs && cargo test`。
- 開発用グローバルリンク: `npm run install-global` / `npm run uninstall-global`。

## コーディング規約
- Rust 2021・スペース4。命名: 関数/モジュールは `snake_case`、型は `CamelCase`。
- ワークスペース Lint: `unwrap/expect` 禁止、`uninlined_format_args` 禁止。
- 変更は最小限・目的特化。無関係リファクタは避け、クレート単位で小さなモジュールへ分割。

## テスト方針
- `cargo test` を基本。ユニットテストは原則同ファイル（`#[cfg(test)]`）、結合は `tests/` も可。
- 命名は挙動起点（例: `renders_help_modal`）。ネットワーク依存は避ける。必要なら `tests/fixtures/` を活用。

## コミット/PR ガイド
- コミット: 命令形サマリ＋必要最小の本文（目的/背景）。例: `tui: add command palette with filter`。
- 1コミット=1論点を推奨。必要に応じてファイル/コマンド例を添付。
- PR: 変更概要/理由/TUI はスクショや asciinema、再現手順、後続タスクを記載。関連 Issue を紐付け、破壊的変更は移行手順も。

## セキュリティ/設定の注意
- 環境によりネットワーク制限あり。極力オフライン動作を担保。
- 書込み先はワークスペース配下（`slides/` や各クレート）。外部書込みは明示の合意がある場合のみ。
- 秘密情報は `slide-common::SlideConfig` 等の設定経由で。ハードコード禁止。

## アーキテクチャ（概観）
- Rust ワークスペースの明確な境界: `core`（セッション/ツール/安全性）、`tui`（ratatui）、`cli`（エントリ）、`common`（設定/IO）、`chatgpt`（HTTP）。`slide-cli` は開発体験のための起動補助。
