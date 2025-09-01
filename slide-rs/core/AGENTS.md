# リポジトリガイドライン

## プロジェクト構成とモジュール
- 目的: エージェント中核（セッション、ツール、コマンド安全性、実行、MCP 連携）。
- クレート: `slide-core`（ライブラリ）。
- 主なモジュール:
  - `codex.rs`（セッション/イベント、承認、実行/パッチ適用フロー）
  - `client.rs`, `client_common.rs`（モデルストリーム IF）
  - `openai_tools.rs`, `plan_tool.rs`, `tool_apply_patch.rs`
  - `apply_patch.rs`（安全判定）、`is_safe_command.rs`（安全サブセットパーサ）
  - `exec_env.rs`, `mcp_connection_manager.rs`, `turn_diff_tracker.rs`

## ビルド/テスト/開発コマンド
- ビルド: `cd slide-rs && cargo build -p slide-core`
- テスト: `cd slide-rs && cargo test -p slide-core`
- 簡易デモ: `client.rs` の `StubClient` でストリーム動作を確認。

## コーディング規約
- Rust 2021。小さく焦点の合ったモジュール。ホットパスでのブロッキング I/O は避ける。
- パニック回避、型付きエラー返却、`tracing` でロギング。
- ポリシー（安全性/サンドボックス）とメカニズム（exec/apply_patch）を分離。

## テスト方針
- 安全判定/パーサ受入/拒否、パッチ評価をユニットテスト。
- Stub クライアントで軽量なストリーム試験を追加。
- イベント順序を検証: SessionConfigured → TaskStarted → delta → Completed。

## 備考
- Exec は初期はシミュレーション。実サンドボックス/実行は段階的に統合。
- 外部 API は安定維持（`Codex::spawn/submit/next_event`）。
- MCP/ツールはフックあり。必要に応じて feature flag で拡張。

## 主要 API（抜粋）
- `codex::Codex::spawn(client) -> CodexSpawnOk { codex, session_id }`
- `codex::Codex::submit(op) -> id`, `codex::Codex::next_event()`
- `apply_patch::assess_patch_safety`, `tool_apply_patch::tool_apply_patch`
