# リポジトリガイドライン

## プロジェクト構成とモジュール
- 目的: ユーザー向け CLI エントリ（`slide`）。
- クレート: `slide-cli`（バイナリ）。
- 主要ファイル: `src/main.rs`（clap コマンド、TUI/core へのディスパッチ）。

## ビルド/テスト/開発コマンド
- 実行: `cd slide-rs/cli && cargo run -- slide` または `npm run dev`
- ビルド: `cd slide-rs && cargo build -p slide-cli`
- テスト: `cd slide-rs && cargo test -p slide-cli`
- 開発用グローバルリンク: `npm run install-global`（ランチャー経由起動）。

## コーディング規約
- コマンド/フラグは Clap derive。サブコマンドは凝集度を保つ。
- `anyhow::Result` を返し、ユーザー向けに簡潔なエラーへ変換。
- ワークスペース配下のパスを尊重し、`slides/` 等以外への書込みは避ける。

## テスト方針
- コマンドフロー（help/preview/interactive）の結合テストを追加。
- テストは密閉（ネットワーク非依存）。必要に応じて CLI help のスナップショットを活用。

## 備考
- インタラクティブは `slide-tui`、エージェント処理は `slide-core` に委譲。
- 致命的エラーは非ゼロ終了とし、解決のヒントを短く提示する。
