## protocol/src ディレクトリの機能概要

このディレクトリは「プロトコル（通信仕様）」を定義する専用クレートです。エージェントとクライアント（CLI/TUI など）の間でやり取りされる型・イベント・メッセージの形を統一し、ワークスペース内の他クレート（core/tui/cli 等）が同じ前提で連携できるようにしています。

### ファイル構成と役割

- `lib.rs`
  - このクレートのエントリーポイント。下位モジュール（`config_types`, `custom_prompts`, `message_history`, `models`, `parse_command`, `plan_tool`, `protocol`）を公開（`pub use`）します。
  - 実体は各モジュール側にあり、`lib.rs` は“ハブ”として機能します。

- `protocol.rs`
  - エージェントとクライアント間の“ワイヤ型（Wire Types）”の本体です。
  - 代表的な要素:
    - `Submission` と `Op`: クライアント→エージェントへ送るリクエスト（サブミッション）とその操作種別。
    - `Event` と `EventMsg`: エージェント→クライアントへ送るイベント（進捗・出力・承認要求・エラー等）。
    - `AskForApproval`, `SandboxPolicy`: 実行の安全性ポリシー、サンドボックス設定。
    - 実行系イベント: `ExecCommandBegin/OutputDelta/End`、パッチ適用 `PatchApplyBegin/End`、承認要求 `ExecApprovalRequest`/`ApplyPatchApprovalRequest` など。
    - セッション系: `SessionConfigured`、`TaskStarted`、`TaskComplete`、`TurnAborted`、`TurnDiff`、`TokenUsage` など。

- `models.rs`
  - 会話やレスポンスの表現を担うデータモデル群。
  - 代表的な要素:
    - `ResponseItem`, `ResponseInputItem`, `ContentItem`: メッセージや関数呼び出しの入出力、推論テキストなどの表示・保持形式。
    - `FunctionCallOutputPayload`: OpenAI Responses API 互換のシリアライズ仕様（出力は常に文字列）を実現。
    - シェル実行関連の型: `LocalShellStatus`, `LocalShellAction`, `LocalShellExecAction`。
    - Web 検索アクション: `WebSearchAction`。

- `config_types.rs`
  - 設定に関する共通型を定義。
  - 代表的な要素:
    - `ReasoningEffort`, `ReasoningSummary`: 推論モデルの出力粒度・要約方針を表す列挙型。
    - `SandboxMode`, `ConfigProfile`: セッションやプロファイルに紐づく実行モードや構成の骨組み（必要に応じて拡張）。

- `custom_prompts.rs`
  - カスタムプロンプトの型を定義。
  - 代表的な要素:
    - `CustomPrompt { name, path, content }`: 名前・格納パス・内容を保持。

- `message_history.rs`
  - 会話履歴の最小単位を定義。
  - 代表的な要素:
    - `HistoryEntry { session_id, ts, text }`: セッション識別子・タイムスタンプ・本文。

- `parse_command.rs`
  - モデルが生成するコマンドの分類（静的パース結果）を表現。
  - 代表的な要素:
    - `ParsedCommand`: `read`, `list_files`, `search`, `format`, `test`, `lint`, `noop`, `unknown` など、用途別のバリアントを持つ列挙型。

- `plan_tool.rs`
  - TODO/プラン更新用の共通引数型を定義。
  - 代表的な要素:
    - `StepStatus`: `pending`/`in_progress`/`completed` の 3 状態。
    - `PlanItemArg`, `UpdatePlanArgs`: プランのアイテムと一括更新ペイロード。

### 全体像（データフロー）

1. クライアントは `Submission { op: Op::... }` を送信
2. エージェントは処理を進めながら `Event { msg: EventMsg::... }` をストリームで返却
3. 実行コマンド・パッチ適用は安全性ポリシー（`AskForApproval`, `SandboxPolicy`）に従い、必要時に承認イベントを発行
4. UI/TUI は `EventMsg` を購読し、チャット出力（`AgentMessage*`）やログ、承認モーダル、差分表示などを反映

この“通信仕様”を共通化することで、エージェント側の実装（core）と UI 側（tui/cli）が疎結合になり、機能追加や実装差し替えが容易になります。


