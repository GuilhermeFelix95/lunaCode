//! CLIs de terminal (Claude Code e Codex): detecção e instalação pelos instaladores oficiais.
//! A URL do script é fixa aqui; nunca vem da tela nem de arquivo, e o app não instala nada sem clique do usuário.

use std::env;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

use crate::providers::ProviderKind;
use crate::session::resolve_in;

/// Script oficial de instalação de cada CLI, no Windows. Os dois redirecionam para o servidor do fornecedor.
/// O OpenCode não tem um script oficial verificado: sem URL, o app não instala e pede a instalação manual.
fn install_url(kind: ProviderKind) -> Option<&'static str> {
    match kind {
        ProviderKind::Claude => Some("https://claude.ai/install.ps1"),
        ProviderKind::Codex => Some("https://chatgpt.com/codex/install.ps1"),
        ProviderKind::OpenCode | ProviderKind::Cline | ProviderKind::Copilot | ProviderKind::Antigravity | ProviderKind::Hermes => None,
    }
}

fn program(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Claude => "claude",
        ProviderKind::Codex => "codex",
        ProviderKind::OpenCode => "opencode",
        ProviderKind::Cline => "cline",
        ProviderKind::Copilot => "copilot",
        ProviderKind::Antigravity => "agy",
        ProviderKind::Hermes => "hermes",
    }
}

fn display_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Claude => "Claude Code",
        ProviderKind::Codex => "Codex",
        ProviderKind::OpenCode => "OpenCode",
        ProviderKind::Cline => "Cline",
        ProviderKind::Copilot => "GitHub Copilot",
        ProviderKind::Antigravity => "Antigravity",
        ProviderKind::Hermes => "Hermes",
    }
}

/// Pastas onde os instaladores oficiais deixam a CLI. Valem mesmo sem reiniciar o app,
/// porque o PATH do processo não muda quando o instalador o altera.
/// Codex: `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin` (script oficial). Claude: `%USERPROFILE%\.local\bin`
/// (o script baixa para `.claude\downloads` e o `claude install` cria o atalho; esse caminho ainda não foi confirmado).
pub fn known_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        // Antigravity (agy): a pasta do instalador oficial, que nem sempre entra no PATH do app.
        dirs.push(PathBuf::from(&local).join("agy").join("bin"));
        // GitHub Copilot (winget): o pacote vai para uma pasta com a versão no nome, e o atalho em Links nem sempre existe.
        dirs.extend(winget_package_dirs(&PathBuf::from(&local).join("Microsoft").join("WinGet").join("Packages"), "GitHub.Copilot_"));
        dirs.push(PathBuf::from(local).join("Programs").join("OpenAI").join("Codex").join("bin"));
    }
    if let Some(home) = env::var_os("USERPROFILE") {
        dirs.push(PathBuf::from(home).join(".local").join("bin"));
    }
    dirs
}

/// Pastas de pacote do winget cujo nome começa com `prefix`. O nome leva a versão e um sufixo, então não dá para fixar.
fn winget_package_dirs(packages: &std::path::Path, prefix: &str) -> Vec<PathBuf> {
    std::fs::read_dir(packages)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        .map(|entry| entry.path())
        .collect()
}

/// Procura `program` no PATH e depois nas pastas conhecidas dos instaladores.
pub fn find(program: &str, path_dirs: &[PathBuf], known: &[PathBuf]) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = path_dirs.iter().chain(known.iter()).cloned().collect();
    resolve_in(program, &dirs, cfg!(windows))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliStatus {
    pub provider: ProviderKind,
    pub installed: bool,
    /// Onde a CLI foi achada. Ausente = não instalada.
    pub path: Option<String>,
}

pub fn status(kind: ProviderKind) -> CliStatus {
    let path_dirs: Vec<PathBuf> = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default();
    let found = find(program(kind), &path_dirs, &known_dirs());
    CliStatus {
        provider: kind,
        installed: found.is_some(),
        path: found.map(|p| p.display().to_string()),
    }
}

/// Roda o instalador oficial da CLI. Só instala se ela ainda não estiver no computador.
pub fn install(kind: ProviderKind) -> Result<CliStatus, String> {
    let before = status(kind);
    if let Some(path) = &before.path {
        return Err(format!("o {} já está instalado em {path}", display_name(kind)));
    }
    let url = install_url(kind).ok_or_else(|| {
        format!("o {} ainda não tem instalador pelo app. Instale o CLI pelo site oficial dele e tente de novo.", display_name(kind))
    })?;
    let script = format!("irm {url} | iex");
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script]);
    #[cfg(windows)]
    {
        // Sem janela de console extra: a saída volta para o app.
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|e| format!("não foi possível rodar o instalador do {}: {e}", display_name(kind)))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "o instalador do {} falhou: {}",
            display_name(kind),
            last_line(&stderr)
        ));
    }
    Ok(status(kind))
}

fn last_line(text: &str) -> String {
    text.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("sem detalhes").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("lunaharness-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn install_scripts_come_from_the_official_hosts() {
        assert!(install_url(ProviderKind::Claude).unwrap().starts_with("https://claude.ai/"));
        assert!(install_url(ProviderKind::Codex).unwrap().starts_with("https://chatgpt.com/"));
        assert!(install_url(ProviderKind::OpenCode).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn finds_a_cli_in_a_known_install_dir_when_the_path_misses_it() {
        let path_dir = temp_dir("path-empty");
        let known_dir = temp_dir("known");
        std::fs::write(known_dir.join("codex.exe"), b"").unwrap();

        let found = find("codex", &[path_dir], std::slice::from_ref(&known_dir));

        assert_eq!(found, Some(known_dir.join("codex.exe")));
    }

    /// O pacote do winget leva a versão no nome: a pasta do Copilot é achada pelo prefixo, e as outras ficam de fora.
    #[test]
    fn winget_package_dirs_match_by_prefix_only() {
        let packages = temp_dir("winget");
        for name in ["GitHub.Copilot_Microsoft.Winget.Source_8wekyb3d8bbwe", "Outro.App_x"] {
            std::fs::create_dir_all(packages.join(name)).unwrap();
        }

        let found = winget_package_dirs(&packages, "GitHub.Copilot_");

        assert_eq!(found, vec![packages.join("GitHub.Copilot_Microsoft.Winget.Source_8wekyb3d8bbwe")]);
    }

    #[cfg(windows)]
    #[test]
    fn the_path_wins_over_the_known_dirs() {
        let path_dir = temp_dir("path-first");
        let known_dir = temp_dir("known-second");
        std::fs::write(path_dir.join("claude.exe"), b"").unwrap();
        std::fs::write(known_dir.join("claude.exe"), b"").unwrap();

        let found = find("claude", std::slice::from_ref(&path_dir), &[known_dir]);

        assert_eq!(found, Some(path_dir.join("claude.exe")));
    }

    #[test]
    fn a_missing_cli_is_not_found() {
        let dir = temp_dir("missing");

        assert_eq!(find("nao-existe-luna", std::slice::from_ref(&dir), std::slice::from_ref(&dir)), None);
    }

    #[test]
    fn the_error_line_skips_blank_lines() {
        assert_eq!(last_line("primeira\n\nfalhou aqui\n\n"), "falhou aqui");
        assert_eq!(last_line(""), "sem detalhes");
    }
}
