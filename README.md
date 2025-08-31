# Slide CLI - ターミナルベースAIエージェント

slide-code.mdの要件に基づいて構築された、チャット形式でMarkdownスライドを生成するRust製CLIツールです。

## 🚀 機能

- **対話的スライド生成**: ターミナル上でチャット形式でスライドを作成
- **TUIプレビュー**: ratatui を使用したターミナル内でのスライドプレビュー
- **クロスプラットフォーム**: Node.jsランチャー経由でネイティブRustバイナリを実行
- **設定管理**: APIキーや承認モードの設定
- **安全な実行**: サンドボックス環境での実行サポート

## 📁 プロジェクト構造

```
slide-code-test/
├── slide-cli/              # Node.js ランチャー
│   ├── bin/slide.js        # プラットフォーム判定・バイナリ起動
│   └── package.json        # npm package定義
├── slide-rs/               # Rust ワークスペース
│   ├── cli/                # メインCLI
│   ├── tui/                # ターミナルUI
│   ├── core/               # コア機能
│   ├── common/             # 共通ユーティリティ
│   ├── chatgpt/            # OpenAI API連携
│   ├── ansi-escape/        # ANSI to ratatui変換
│   ├── arg0/               # バイナリディスパッチ
│   └── apply-patch/        # パッチ適用
└── slides/                 # 生成されたスライド
    └── sample.md
```

## ⚙️ 技術スタック

### Rust (バックエンド)
- **ratatui 0.29** - ターミナルUI
- **tokio 1.47** - 非同期処理
- **clap 4.5** - コマンドライン引数処理
- **serde** - シリアライズ/デシリアライズ
- **reqwest** - HTTP通信 (OpenAI API)

### Node.js (ランチャー)
- **ES Modules** - モダンJavaScript
- **@vscode/ripgrep** - テキスト検索
- プラットフォーム別バイナリ実行

## 📦 インストール

### グローバルインストール (推奨)
```bash
npm install -g @taiyo/slide
slide
```

### ローカル開発
```bash
# リポジトリをクローン
git clone https://github.com/kimurataiyou/slide-code-test.git
cd slide-code-test

# 簡単実行
npm run dev
# または
./slide.sh
```

## 🎯 使用方法

### 1. 対話モード
```bash
slide
# ターミナル内でTUIが起動し、チャット形式でスライドを作成
```

### 2. プレビューモード
```bash
slide preview slides/sample.md
# Markdownスライドをターミナル内でプレビュー
# ←/→ または j/k でナビゲーション、qで終了
```

### 3. ワンショットモード (予定)
```bash
slide "営業向け提案 10枚 日本語で"
# プロンプト一発でスライドを生成
```

## 🛠️ 開発

### ローカル実行コマンド
```bash
npm run dev     # slide.sh経由でローカル実行
npm run build   # Rustバイナリをビルド  
npm run test    # Rustテスト実行
```

### 手動ビルド
```bash
cd slide-rs
cargo build --release
```

### テスト
```bash
cargo check  # コンパイルチェック
cargo test   # テスト実行
```

### グローバルインストール (開発用)
```bash
npm run install-global    # ローカルからグローバルにリンク
npm run uninstall-global  # グローバルリンクを解除
```

## 📋 実装状況

### ✅ 完了
- [x] Cargo ワークスペース構造
- [x] Node.js ランチャー
- [x] 基本CLI構造
- [x] TUI プレビュー機能
- [x] プラットフォーム別バイナリ対応
- [x] 警告修正・コードクリーンアップ

### 🚧 今後の拡張
- [ ] OpenAI API実装 (現在はMock)
- [ ] login/logout コマンド
- [ ] ワンショット生成モード
- [ ] 設定ファイル管理
- [ ] 承認フロー (suggest/auto-edit/full-auto)
- [ ] 画像生成・埋め込み
- [ ] PPTX/PDF変換連携

## 🎨 アスキーアート（実装完了記念）

```
=== 編集前 ===
     🌱
   (空のプロジェクト)

=== 編集後 ===
       🏗️
    📊 Slide CLI 🚀
   ┌─────────────────┐
   │  slide-cli/     │
   │  ├─ Node.js     │
   │  └─ Launcher    │
   └─────────────────┘
          │
          ▼
   ┌─────────────────┐
   │  slide-rs/      │
   │  ├─ Rust TUI    │
   │  ├─ ChatGPT     │
   │  ├─ Core Logic  │
   │  └─ Preview     │
   └─────────────────┘
          │
          ▼
   ┌─────────────────┐
   │   slides/       │
   │ 📝 sample.md    │
   └─────────────────┘
```

## 🤖 AIエージェント機能

このプロジェクトは slide-code.md の要件定義に基づき、Claude Code によって自動生成されました。

- **対話的設計**: ユーザーの要求に基づく段階的な実装
- **モジュラー構造**: 各機能を独立したクレートに分離
- **最新技術**: Rust 2021 Edition、最新のratatui使用
- **クロスプラットフォーム**: macOS/Linux/Windowsサポート準備済み

---

*🤖 Generated with [Claude Code](https://claude.ai/code)*

*Co-Authored-By: Claude <noreply@anthropic.com>*