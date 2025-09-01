# リポジトリガイドライン

## プロジェクト構成とモジュール
- 目的: スライド生成に用いる ChatGPT 系プロバイダ向け HTTP クライアント。
- クレート: `slide-chatgpt`（ライブラリ）。
- 主なファイル: `src/lib.rs`（クライアント型、リクエスト/レスポンス処理）。

## ビルド/テスト/開発コマンド
- ビルド: `cd slide-rs && cargo build -p slide-chatgpt`
- テスト: `cd slide-rs && cargo test -p slide-chatgpt`
- 実行例（擬似）:
  - `let client = ChatGptClient::new(api_key); client.generate_slides(req).await?;`

## コーディング規約
- Rust 2021・4スペース。ブロッキング I/O を避け（`tokio`+`reqwest`）。
- リクエストは強い型で表現し `serde` でシリアライズ。
- HTTP エラーは `anyhow` に文脈付きでマップ（可能なら status/body を含む）。

## テスト方針
- モック HTTP を優先。CI ではネットワークを避ける。
- タイムアウト、エラーマッピング、リトライ/バックオフを検証。
- JSON 形式ずれ（欠落/未知フィールド）をカバー。

## 備考
- API キーはコードに含めない。設定/環境変数から取得。
- プロバイダ非依存・最小実装を維持。
- 今後: ストリーミング API、レート制御/バックオフ、設定によるプロバイダ選択。
