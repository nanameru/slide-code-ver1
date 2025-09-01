# リポジトリガイドライン

## プロジェクト構成とモジュール
- 目的: エージェントが用いる制約付き・監査可能なパッチ適用。
- クレート: `slide-apply-patch`（ライブラリ）。
- 主なファイル:
  - `src/lib.rs`: パッチパーサ/適用器。ヘッダ `*** Add/Update/Delete File:` と `+` 行に対応。
  - 挙動: `Update` は `+` 行を結合して全置換（MVP）。コンテキスト/`-` 行は無視。

## ビルド/テスト/開発コマンド
- ビルド: `cd slide-rs && cargo build -p slide-apply-patch`
- テスト: `cd slide-rs && cargo test -p slide-apply-patch`
- 例:
  - プログラムから適用: `slide_apply_patch::apply_patch_to_files(patch, false)?;`

## コーディング規約
- Rust 2021・4スペース。ライブラリでは `unwrap/expect` を避ける。
- I/O は局所化し、ディレクトリ作成を適切に行う。
- 入力は防御的に検証。未知ヘッダの扱いは呼出側の要件に合わせる。

## テスト方針
- Add/Update/Delete の流れを検証し、内容と冪等性を確認。
- 無効ヘッダや不完全 hunk を含め、安全な失敗を確認。
- 一時ディレクトリを使用し、ディレクトリ作成や UTF-8/BOM の扱いを検証。

## 備考
- `slide-core` の apply_patch ツールから利用。対象ファイル以外への副作用を避ける。
- 今後: hunk 対応、Move/Rename のネイティブ適用、dry-run 要約、差分レポートの充実。
