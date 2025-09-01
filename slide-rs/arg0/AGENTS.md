# リポジトリガイドライン

## プロジェクト構成とモジュール
- 目的: 正しい argv[0] でプロセスを起動するためのヘルパ。
- クレート: `slide-arg0`（ライブラリ）。
- 主要ファイル: `src/lib.rs`。

## ビルド/テスト/開発コマンド
- ビルド: `cd slide-rs && cargo build -p slide-arg0`
- テスト: `cd slide-rs && cargo test -p slide-arg0`

## コーディング規約
- Rust 2021・4スペース。プラットフォーム依存の思い込みを避ける。
- 関数は小さく、失敗のある操作は `anyhow::Result` を返す。

## テスト方針
- argv 合成や OS 差分に関するユニットテストを追加（可能な範囲で）。

## 備考
- サブコマンドやサンドボックス補助を呼び出す際に CLI/core から利用。

## 例
- 実行引数の合成:
  - `let argv = slide_arg0::compose("/usr/bin/env", &["bash", "-lc", "echo hi"])`（実際の API に合わせて調整）。
- Windows/Unix の分岐は `cfg(target_os)` で適切に切り分け。
