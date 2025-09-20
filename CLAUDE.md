# Slide Code Test - 開発ドキュメント

## 🔧 最新の修正履歴
- **2024-09-20**: GPT-5モデル設定を修正、デフォルトモデルをgpt-4o-miniからGPT-5に変更
- **2024-09-20**: AI Tool Execution機能を実装、codexシステムと同等の機能を追加（MCP対応を除く）
- **2024-09-20**: TUIディスプレイ機能を強化、カラーコード差分表示、チェックボックス、セクションヘッダーを追加
- **2024-09-20**: ストリーミング出力の書式設定を改善、リアルタイムの色分け表示を実装

## 🏗️ プロジェクトの目的と責務

**Slide Code Test**は、ターミナルベースのAIエージェントとして動作するRust製CLIツールです。主な機能：

1. **対話的スライド生成**: チャット形式でMarkdownスライドを作成
2. **TUIプレビュー**: ratatui を使用したターミナル内でのスライドプレビュー
3. **AI Tool Execution**: コマンド実行、ファイル操作、パッチ適用などのツール実行機能
4. **サンドボックス実行**: 安全なコマンド実行環境
5. **クロスプラットフォーム**: Node.jsランチャー経由でネイティブRustバイナリを実行

## 📁 主要ディレクトリ構造

```
slide-code-test/
├── slide-rs/               # メインRustワークスペース
│   ├── cli/                # メインCLIエントリーポイント
│   ├── tui/                # ターミナルUI（ratatui使用）
│   ├── core/               # コア機能・ツール実行エンジン
│   ├── chatgpt/            # OpenAI API連携（GPT-5対応）
│   ├── common/             # 共通ユーティリティ
│   ├── ansi-escape/        # ANSI to ratatui変換
│   ├── apply-patch/        # パッチ適用ツール
│   ├── arg0/               # バイナリディスパッチ
│   ├── protocol/           # 通信プロトコル
│   └── slides-tools/       # スライド生成ツール
├── slide-cli/              # Node.jsランチャー
├── core/                   # レガシーコア（今後統合予定）
├── chatgpt/                # レガシーChatGPT（今後統合予定）
└── slides/                 # 生成されたスライド
```

## ⚙️ 技術的な実装詳細

### Core Technologies
- **Rust 2021 Edition**: メイン実装言語
- **ratatui 0.29**: ターミナルUI框架
- **tokio**: 非同期処理
- **OpenAI API**: GPT-5モデル使用
- **serde**: シリアライズ/デシリアライズ
- **clap**: CLI引数処理

### 重要な実装ファイル

#### `/slide-rs/core/src/` - コア機能
- **`exec.rs`**: メインツール実行エンジン、サンドボックス実行、承認フロー
- **`shell.rs`**: シェル設定（Zsh/PowerShell対応）
- **`is_safe_command.rs`**: コマンド安全性チェック、許可リスト管理
- **`tool_executor.rs`**: ツール実行オーケストレーション
- **`tool_apply_patch.rs`**: パッチ適用ツール
- **`openai_tools.rs`**: OpenAI Functions定義

#### `/slide-rs/chatgpt/src/client.rs` - AI API連携
- **デフォルトモデル**: `gpt-5` (最新修正済み)
- **ストリーミング対応**: リアルタイム応答
- **Function Calling**: ツール実行機能統合

#### `/slide-rs/tui/src/` - ターミナルUI
- **`widgets/chat.rs`**: チャットウィジェット、ツール実行結果の色分け表示
- **`streaming.rs`**: ストリーミング出力のリアルタイム書式設定

### AI Tool Execution 機能

codexシステムと同等の以下のツール実行機能を実装：

1. **Exec Tool**: シェルコマンド実行
   - サンドボックス実行環境
   - 安全性チェック（허가リスト方式）
   - 承認フロー（危険コマンドの事前確認）
   - ストリーミング出力

2. **Apply Patch Tool**: ファイルパッチ適用
   - unified diff形式対応
   - バックアップ機能
   - エラーハンドリング

3. **Display Formatting**: ツール実行結果の視覚的表示
   - **差分表示**: `+`行（緑）、`-`行（赤）、`@@`行（青）
   - **セクションヘッダー**: "Updated Plan"（青）、"Proposed Change"（黄）、"Change Approved"（緑）
   - **チェックボックス**: `□`（グレー）、`☑`（緑）
   - **ファイルパス**: `.rs`, `.toml`, `.md`ファイル（ライトブルー）

## 🎨 実装パターンとベストプラクティス

### エラーハンドリング
```rust
// anyhow::Result を統一的に使用
pub async fn process_exec_tool_call(
    params: ExecParams,
    // ...
) -> Result<ExecToolCallOutput>
```

### 非同期処理パターン
```rust
// tokio::process::Command を使用
let mut child = tokio::process::Command::new(&command[0])
    .args(&command[1..])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
```

### TUIカラーコード
```rust
// ratatui::style::Color を使用した統一的な色分け
Style::default().fg(Color::Green)    // 成功・追加
Style::default().fg(Color::Red)      // エラー・削除
Style::default().fg(Color::Blue)     // 情報・ヘッダー
Style::default().fg(Color::Yellow)   // 警告・変更提案
```

## 👥 開発ガイドライン

### ビルド＆実行コマンド
```bash
# 開発実行
npm run dev
# または
./slide.sh

# Rustワークスペースビルド
cd slide-rs && cargo build --release

# テスト実行
cargo test

# 型チェック
cargo check
```

### TUI実行コマンド
```bash
# メインTUIアプリ
slide

# スライドプレビュー
slide preview slides/sample.md
```

### 環境変数設定
```bash
# .env.local または env.local に設定
OPENAI_API_KEY=your_api_key_here
SLIDE_APP=1  # Slideモード有効化
```

### セキュリティポリシー

#### 安全なコマンド（自動実行可能）
- 読み取り専用: `ls`, `cat`, `grep`, `head`, `tail`, `pwd`
- Git読み取り: `git status`, `git log`, `git diff`, `git show`
- システム情報: `whoami`, `date`, `uname`, `env`

#### 危険なコマンド（承認必須）
- ファイル操作: `rm`, `mv`, `cp`, `chmod`, `chown`
- ネットワーク: `curl`, `wget`
- 権限昇格: `sudo`, `su`

### コーディング規約
```rust
// unwrap()使用禁止（Clippy設定済み）
unwrap_used = "deny"

// expect()使用禁止
expect_used = "deny"

// Result<T>は必ずハンドリングする
#[must_use]
```

## 🛠️ トラブルシューティング

### よくある問題

#### 1. OpenAI APIキーエラー
```bash
Error: "Missing OPENAI_API_KEY"
```
**解決**: `.env.local`ファイルにAPIキーを設定

#### 2. コンパイルエラー（最新コミット後）
```bash
error[E0433]: failed to resolve: use of undeclared crate or module
```
**解決**: `cargo clean && cargo build`で再ビルド

#### 3. TUI表示崩れ
```bash
# ターミナルサイズをリセット
export TERM=xterm-256color
resize
```

#### 4. ツール実行が動作しない
- サンドボックス設定を確認: `SandboxType::Disabled`でテスト
- ログ確認: `http://127.0.0.1:6060/`でログビューア開く

### デバッグコマンド
```bash
# ログレベル設定
RUST_LOG=debug cargo run

# 詳細ログ出力
RUST_LOG=slide_core=trace cargo run
```

### 依存関係の問題
```bash
# 依存関係更新
cargo update

# 特定クレート再ビルド
cargo build -p slide-core
```

## 📋 今後の開発方針

### 優先度高
- [ ] コンパイルエラーの完全修正
- [ ] MCP（Model Context Protocol）対応
- [ ] ワンショット生成モード実装
- [ ] エラーログ改善

### 優先度中
- [ ] レガシーコードの統合（core/, chatgpt/ディレクトリ）
- [ ] テストカバレッジ向上
- [ ] パフォーマンス最適化

### 優先度低
- [ ] Windows PowerShellサポート強化
- [ ] 設定ファイル管理UI
- [ ] プラグインシステム

---

**🤖 このドキュメントは Claude Code による自動解析に基づいて作成されました**

**Co-Authored-By: Claude <noreply@anthropic.com>**