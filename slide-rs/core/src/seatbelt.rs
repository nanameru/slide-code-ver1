// Re-export from protocol to maintain consistency
pub use protocol::protocol::{SandboxPolicy, WritableRoot};

// Base policy borrowed from codex-1 approach
const MACOS_SEATBELT_BASE_POLICY: &str = include_str!("seatbelt_base_policy.sbpl");

/// 日本語メモ（文系向け）
/// このファイルは、macOSの「サンドボックスのルール用紙（SBPL）」を作る係です。
/// - 読むのは基本OK（file-read*）
/// - 書ける場所は「プロジェクトの島（WritableRoot）」だけ（しかも島の中でもダメなポケットは除外）
/// - ネットは設定でON/OFFできます
/// こうして作ったルール用紙を `sandbox-exec` に渡すことで、AIが実行するコマンドの行動範囲を安全に絞れます。
/// codex-1 と同じ作法で紙を作るので、挙動が揃い安全性が上がります。
///
/// できるようになったこと（要点）:
/// - プロジェクト配下だけにファイル書き込みを限定（誤爆防止）
/// - その中でも「.git など触ってほしくない所」は書き込み禁止
/// - ネットワークを必要に応じてON/OFF
/// - パス表記のゆれを正規化して、意図しない穴を避ける
///
/// Build a macOS Seatbelt SBPL string aligned with codex-1's policy model.
/// - Always allow read (file-read*)
/// - Allow write only under declared writable roots, with read-only subpaths excluded
/// - Optionally allow network (outbound/inbound/system-socket)
pub fn build_seatbelt_policy(policy: SandboxPolicy, cwd: &std::path::Path) -> String {
    match policy {
        SandboxPolicy::DangerFullAccess => {
            // Broadly permissive: keep legacy behavior
            "(version 1)\n(allow default)".to_string()
        }
        SandboxPolicy::ReadOnly => {
            // Base + read-only
            let file_read_policy = "; allow read-only file operations\n(allow file-read*)";
            format!(
                "{base}\n{file_read_policy}\n",
                base = MACOS_SEATBELT_BASE_POLICY,
                file_read_policy = file_read_policy
            )
        }
        SandboxPolicy::WorkspaceWrite {
            network_access,
            ..
        } => {
            // Read policy
            let file_read_policy = "; allow read-only file operations\n(allow file-read*)";

            // Write policy per writable root, excluding read-only subpaths using require-not
            let writable_roots = policy.get_writable_roots_with_cwd(cwd);
            let mut writable_components: Vec<String> = Vec::new();
            for wr in writable_roots.iter() {
                let root_canon = wr
                    .root
                    .canonicalize()
                    .unwrap_or_else(|_| wr.root.clone());
                if wr.read_only_subpaths.is_empty() {
                    writable_components.push(format!(
                        "(subpath \"{}\")",
                        root_canon.to_string_lossy()
                    ));
                } else {
                    let mut parts: Vec<String> = Vec::new();
                    parts.push(format!(
                        "(subpath \"{}\")",
                        root_canon.to_string_lossy()
                    ));
                    for ro in wr.read_only_subpaths.iter() {
                        let ro_canon = ro.canonicalize().unwrap_or_else(|_| ro.clone());
                        parts.push(format!(
                            "(require-not (subpath \"{}\"))",
                            ro_canon.to_string_lossy()
                        ));
                    }
                    writable_components.push(format!("(require-all {} )", parts.join(" ")));
                }
            }

            let file_write_policy = if writable_components.is_empty() {
                String::new()
            } else {
                format!("(allow file-write*\n{}\n)", writable_components.join(" "))
            };

            // Network policy (more precise than legacy network*)
            let network_policy = if network_access {
                "(allow network-outbound)\n(allow network-inbound)\n(allow system-socket)"
                    .to_string()
            } else {
                String::new()
            };

            format!(
                "{base}\n{file_read}\n{file_write}\n{network}\n",
                base = MACOS_SEATBELT_BASE_POLICY,
                file_read = file_read_policy,
                file_write = file_write_policy,
                network = network_policy
            )
        }
    }
}
