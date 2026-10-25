//! Navegador do Playwright para o agente: alternativa ao navegador embutido. O agente recebe um ou outro, nunca os dois.
//! O servidor é o pacote oficial `@playwright/mcp`, fixado na versão testada.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::logfile;
use crate::session::{hidden_command, resolve_program};

/// Pacote oficial do servidor MCP do Playwright, na versão testada com o OpenCode e o Codex.
pub const PACKAGE: &str = "@playwright/mcp@0.0.83";

/// Ferramentas do Playwright já aprovadas no Claude (sem pergunta), como o navegador embutido.
pub const CLAUDE_ALLOWED: &str = "mcp__playwright";

static CONFIG: OnceLock<PathBuf> = OnceLock::new();

/// Programa e argumentos que sobem o servidor. No Windows o `npx` é um `.cmd`, que o CLI não executa direto.
pub fn server_command() -> (&'static str, Vec<&'static str>) {
    if cfg!(windows) {
        ("cmd", vec!["/c", "npx", "-y", PACKAGE])
    } else {
        ("npx", vec!["-y", PACKAGE])
    }
}

/// Configuração MCP no formato do Claude Code (`--mcp-config`).
fn claude_config() -> Value {
    let (command, args) = server_command();
    json!({ "mcpServers": { "playwright": { "type": "stdio", "command": command, "args": args } } })
}

/// Grava a configuração do Claude em `app_data`. Falha só vira registro: o app abre sem o Playwright.
pub fn start(app_data: &Path) -> Result<(), String> {
    std::fs::create_dir_all(app_data).map_err(|e| e.to_string())?;
    let path = app_data.join("playwright-mcp.json");
    let text = serde_json::to_string_pretty(&claude_config()).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("não foi possível gravar {}: {e}", path.display()))?;
    let _ = CONFIG.set(path);
    // O agente recebe o Playwright por execução (`mcp_config`); o registro global só gastaria tokens em toda sessão.
    crate::mcp::desligar_global_no_inicio(app_data, "playwright")
}

/// Arquivo de configuração do Claude, se o app conseguiu gravá-lo.
pub fn mcp_config() -> Option<&'static Path> {
    CONFIG.get().map(PathBuf::as_path)
}

/// Há um Chromium do Playwright baixado no computador (pasta `ms-playwright` do usuário, no Windows).
pub fn chromium_installed() -> bool {
    let Some(base) = std::env::var_os("LOCALAPPDATA") else {
        return false;
    };
    std::fs::read_dir(PathBuf::from(base).join("ms-playwright"))
        .map(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().starts_with("chromium-")))
        .unwrap_or(false)
}

/// Baixa o Chromium do Playwright. Só roda depois que a pessoa confirma na tela.
pub fn install_chromium() -> Result<(), String> {
    let program = resolve_program("npx").ok_or("npx não encontrado no PATH. Instale o Node.js para usar o Playwright.")?;
    let output = hidden_command(&program)
        .args(["-y", "playwright", "install", "chromium"])
        .output()
        .map_err(|e| format!("não foi possível rodar o instalador do Playwright: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = format!(
        "o instalador do Playwright terminou com código {:?}: {}",
        output.status.code(),
        stderr.trim().chars().rev().take(300).collect::<String>().chars().rev().collect::<String>()
    );
    logfile::error(&message);
    Err(message)
}

#[tauri::command]
pub async fn playwright_status() -> bool {
    tauri::async_runtime::spawn_blocking(chromium_installed).await.unwrap_or(false)
}

#[tauri::command]
pub async fn playwright_install() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(install_chromium)
        .await
        .map_err(|e| format!("instalação do Playwright interrompida: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_config_starts_the_pinned_package_through_the_platform_launcher() {
        let config = claude_config();
        let server = &config["mcpServers"]["playwright"];

        assert_eq!(server["type"], "stdio");
        let args: Vec<&str> = server["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
        assert!(args.contains(&PACKAGE));
        assert!(args.contains(&"-y"));
    }

    #[test]
    fn server_command_is_the_pinned_package_on_any_platform() {
        let (_, args) = server_command();

        assert!(args.contains(&PACKAGE));
    }
}
