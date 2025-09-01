# bottom_pane ディレクトリガイド

## 目的
下部ペイン（コンポーザーや各種モーダル/ポップアップ）を統合し、チャット入力と補助ビューを提供します。

## ファイル構成と役割
- `mod.rs`: 中核。`BottomPane`/`CancellationEvent` を定義。高さ計算、レイアウト、キー処理、Ctrl-C、描画委譲。
- `bottom_pane_view.rs`: インターフェース。`BottomPaneView` トレイト（キー処理、完了、Ctrl-C、望高、描画）。

（将来追加候補）
- `approval_modal_view.rs`: 承認要求モーダル（Yes/No）。
- `list_selection_view.rs`: リスト選択 UI（コマンドパレット/検索結果など）。
- `chat_composer.rs` / `textarea.rs` / `chat_composer_history.rs`: 入力欄/貼付/履歴。
- `command_popup.rs` / `file_search_popup.rs`: コマンド/検索ポップアップ。
- `scroll_state.rs` / `selection_popup_common.rs` / `popup_consts.rs`: 選択 UI 共通部。
- `paste_burst.rs`: 大量貼付のバッファリング/分割反映。

## コーディング指針
- Render（描画）と Update（状態）を分離。副作用最小。
- アクティブビューを優先し、なければコンポーザーにキー委譲。
- `is_complete()` の契約に従い、閉鎖時にビューを破棄。

## テスト指針
- 望高/レイアウト境界（高さ 1/2 行）。
- モーダル優先表示と復帰動作。
- リスト選択のフィルタ/移動/決定。

## 統合ポイント
- `BottomPane::desired_height / render_ref / handle_key_event` を App から利用。
- エージェントイベント（実行/承認/差分）に応じてビュー差し込み・状態更新。
