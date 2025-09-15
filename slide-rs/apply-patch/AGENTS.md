# slide-apply-patch（本格実装）

Codex の `apply_patch` 仕様に準拠したパッチ言語を解析・適用するライブラリ/CLI です。以下の操作に対応します:

- Add: 新規ファイル作成（以降は `+` 行のみ）
- Delete: 既存ファイル削除
- Update: 既存ファイルの部分更新（任意の `*** Move to:` リネームを含む）

## フォーマット（抜粋）
```
*** Begin Patch
*** Add File: path/new.txt
+hello
*** Update File: src/app.rs
@@ fn main()
-println!("Hi")
+println!("Hello")
*** Delete File: old.txt
*** End Patch
```

## 主なモジュール
- `src/parser.rs`: パッチテキストを `Hunk` の列に厳密/寛容パース
- `src/seek_sequence.rs`: コンテキスト行の曖昧一致（空白/Unicode句読点の正規化対応）
- `src/standalone_executable.rs`: CLIエントリ（引数/標準入力の取り回しと実行）
- `src/lib.rs`: 入口・APIおよびファイル適用処理（unified diff生成/要約出力など）

## 使い方（CLI）
```
apply_patch "*** Begin Patch\n*** Add File: hello.txt\n+Hello\n*** End Patch\n"
```
標準入力からも受け付けます:
```
echo "*** Begin Patch..." | apply_patch
```

## ライブラリAPI（概要）
- `apply_patch(patch, stdout, stderr) -> Result<()>`: パース→適用→要約出力
- `parse_patch(patch) -> ApplyPatchArgs`: パッチ構文解析（`Add/Delete/Update`）
- `unified_diff_from_chunks(...)`: 更新結果の unified diff 生成

## 注意
- ファイルパスは相対のみを許容する想定
- 末尾 `*** End of File` による EOF 近接変更にも対応
