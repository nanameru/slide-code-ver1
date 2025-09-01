# リポジトリガイドライン

## プロジェクト構成とモジュール
- 目的: ANSI エスケープ（色/装飾/リンク）を ratatui 向け Span/Cell に変換。
- クレート: `slide-ansi-escape`（ライブラリ）。
- 主なファイル:
  - `src/lib.rs`: 解析/変換のエントリとヘルパ。TUI から利用する小さな API を提供。

## ビルド/テスト/開発コマンド
- ビルド: `cd slide-rs && cargo build -p slide-ansi-escape`
- テスト: `cd slide-rs && cargo test -p slide-ansi-escape`
- 例（擬似）:
  - 変換: `let out = slide_ansi_escape::to_spans("\u001b[31mred\u001b[0m")`。

## コーディング規約
- Rust 2021・4スペース。可能であればタイトループ内の割当を抑制。
- 関数は純粋・決定的に。グローバル/静的状態に依存しない。
- 診断は `tracing` を使用。ライブラリから `println!` は行わない。
- 公開 API は最小限に（`impl IntoIterator<Item=Span>` 型の戻りを推奨）。

## テスト方針
- 機能別にユニットテスト: ネスト SGR、リセット、リンク、全角、長行、分割シーケンス。
- 退行テスト: 難解なシーケンスや no-op（空/素のテキスト）。
- 端末依存を避け、生成 Span/Style を直接比較。

## 備考と境界
- 依存: `ansi-to-tui`, `ratatui`, `tracing`。
- アプリ状態（カーソル/スクロール）に依存しない。入力→描画可能な出力への変換のみに責務を限定。
