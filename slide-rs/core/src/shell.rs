use serde::Deserialize;
use serde::Serialize;
use shlex;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ZshShell {
    shell_path: String,
    zshrc_path: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct PowerShellConfig {
    exe: String, // Executable name or path, e.g. "pwsh" or "powershell.exe".
    bash_exe_fallback: Option<PathBuf>, // In case the model generates a bash command.
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Shell {
    Zsh(ZshShell),
    PowerShell(PowerShellConfig),
    Unknown,
}

impl Shell {
    pub fn format_default_shell_invocation(&self, command: Vec<String>) -> Option<Vec<String>> {
        match self {
            Shell::Zsh(zsh) => {
                if !std::path::Path::new(&zsh.zshrc_path).exists() {
                    return None;
                }

                let mut result = vec![zsh.shell_path.clone()];
                result.push("-lc".to_string());

                let joined = strip_bash_lc(&command)
                    .or_else(|| shlex::try_join(command.iter().map(|s| s.as_str())).ok());

                if let Some(joined) = joined {
                    result.push(format!("source {} && ({joined})", zsh.zshrc_path));
                } else {
                    return None;
                }
                Some(result)
            }
            Shell::PowerShell(ps) => {
                // If model generated a bash command, prefer a detected bash fallback
                if let Some(script) = strip_bash_lc(&command) {
                    return match &ps.bash_exe_fallback {
                        Some(bash) => Some(vec![
                            bash.to_string_lossy().to_string(),
                            "-lc".to_string(),
                            script,
                        ]),

                        // No bash fallback → run the script under PowerShell.
                        // It will likely fail (except for some simple commands), but the error
                        // should give a clue to the model to fix upon retry that it's running under PowerShell.
                        None => Some(vec![
                            ps.exe.clone(),
                            "-NoProfile".to_string(),
                            "-Command".to_string(),
                            script,
                        ]),
                    };
                }

                // Not a bash command. If model did not generate a PowerShell command,
                // turn it into a PowerShell command.
                let first = command.first().map(String::as_str);
                if first != Some(ps.exe.as_str()) {
                    // TODO (CODEX_2900): Handle escaping newlines.
                    if command.iter().any(|a| a.contains('\n') || a.contains('\r')) {
                        return Some(command);
                    }

                    let joined = shlex::try_join(command.iter().map(|s| s.as_str())).ok();
                    return joined.map(|arg| {
                        vec![
                            ps.exe.clone(),
                            "-NoProfile".to_string(),
                            "-Command".to_string(),
                            arg,
                        ]
                    });
                }

                // Model generated a PowerShell command. Run it.
                Some(command)
            }
            Shell::Unknown => None,
        }
    }

    pub fn name(&self) -> Option<String> {
        match self {
            Shell::Zsh(zsh) => std::path::Path::new(&zsh.shell_path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string()),
            Shell::PowerShell(ps) => Some(ps.exe.clone()),
            Shell::Unknown => None,
        }
    }
}

impl Default for Shell {
    fn default() -> Self {
        #[cfg(windows)]
        {
            Shell::PowerShell(PowerShellConfig {
                exe: "powershell.exe".to_string(),
                bash_exe_fallback: None,
            })
        }
        #[cfg(not(windows))]
        {
            Shell::Zsh(ZshShell {
                shell_path: "/bin/zsh".to_string(),
                zshrc_path: format!("{}/.zshrc", std::env::var("HOME").unwrap_or_default()),
            })
        }
    }
}

fn strip_bash_lc(command: &[String]) -> Option<String> {
    if command.len() == 3 && command[0] == "bash" && command[1] == "-lc" {
        Some(command[2].clone())
    } else {
        None
    }
}

// Legacy compatibility
#[derive(Debug, Clone)]
pub struct ShellConfig {
    pub program: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zsh_shell_invocation() {
        let zsh = Shell::Zsh(ZshShell {
            shell_path: "/bin/zsh".to_string(),
            zshrc_path: "/tmp/test_zshrc".to_string(),
        });

        // Create a test zshrc file
        std::fs::write("/tmp/test_zshrc", "# test").unwrap();

        let command = vec!["echo".to_string(), "hello".to_string()];
        let result = zsh.format_default_shell_invocation(command);

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result[0], "/bin/zsh");
        assert_eq!(result[1], "-lc");
        assert!(result[2].contains("source /tmp/test_zshrc"));

        // Clean up
        std::fs::remove_file("/tmp/test_zshrc").ok();
    }

    #[test]
    fn test_bash_lc_detection() {
        let command = vec!["bash".to_string(), "-lc".to_string(), "ls".to_string()];
        let result = strip_bash_lc(&command);
        assert_eq!(result, Some("ls".to_string()));
    }
}
