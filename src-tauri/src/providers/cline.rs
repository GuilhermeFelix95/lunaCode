use std::path::{Path, PathBuf};

use serde_json::Value;

use super::ProviderEvent;

/// `cline --json` com o prompt num arquivo. O Cline só aceita o prompt como argumento (o stdin não serve no modo JSON),
/// então o app grava o prompt num arquivo e passa uma instrução curta para o CLI lê-lo.
/// Sem `-y` e sem `--plan`: no modo JSON com saída redirecionada, o Cline executa comandos sem pedir aprovação.
pub fn args(model: Option<&str>, prompt_file: &Path) -> Vec<String> {
    let mut args: Vec<String> = vec!["--json".into()];
    if let Some(model) = model {
        args.extend(["-m".to_string(), model.to_string()]);
    }
    args.push(format!("Leia o arquivo {} e siga as instruções nele.", prompt_file.display()));
    args
}

/// Grava o prompt num arquivo temporário e devolve o caminho. O nome leva um carimbo de tempo, para não sobrescrever outro turno.
pub fn write_prompt(prompt: &str) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    let path = std::env::temp_dir().join(format!("lunaharness-cline-{stamp}.md"));
    std::fs::write(&path, prompt).map_err(|e| format!("não foi possível gravar o prompt do Cline em {}: {e}", path.display()))?;
    Ok(path)
}

/// Login do Cline: o `cline auth` guarda o provedor em `<pasta>/data/settings/providers.json`.
/// A pasta é a da conta (`CLINE_DIR`) ou, sem conta, `~/.cline`. Só se lê se há credencial para o provedor escolhido; o conteúdo não sai daqui.
pub fn logged_in_at(base: Option<&str>) -> bool {
    let root = match base {
        Some(dir) => PathBuf::from(dir),
        None => {
            let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) else {
                return false;
            };
            PathBuf::from(home).join(".cline")
        }
    };
    let path = root.join("data").join("settings").join("providers.json");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    serde_json::from_str::<Value>(&raw)
        .ok()
        .and_then(|v| {
            let provider = v["lastUsedProvider"].as_str()?;
            let auth = &v["providers"][provider]["settings"]["auth"];
            Some(auth["accessToken"].is_string() || auth["apiKey"].is_string())
        })
        .unwrap_or(false)
}

/// Detalhe curto da ferramenta: o comando ou o caminho do arquivo lido.
fn tool_detail(name: &str, input: &Value) -> String {
    match name {
        "run_commands" => input["commands"]
            .as_array()
            .map(|commands| commands.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; "))
            .unwrap_or_default(),
        "read_files" => input["files"]
            .as_array()
            .map(|files| files.iter().filter_map(|f| f["path"].as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Uma linha do `cline --json`. Formato capturado com a CLI 3.0.73: `agent_event` com `content_start`/`content_end`
/// (texto e ferramenta), `run_result` no fim com o modelo que respondeu, e `error` quando a execução falha.
/// Eventos de hook e de iteração não viram nada no chat.
pub fn parse_line(line: &str) -> Vec<ProviderEvent> {
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            return vec![ProviderEvent::Error {
                message: "cline: linha de saída não é JSON válido".into(),
            }]
        }
    };
    match value["type"].as_str() {
        Some("agent_event") => agent_event(&value["event"]),
        Some("run_result") => {
            let mut events = Vec::new();
            if let Some(model) = value["model"]["id"].as_str() {
                events.push(ProviderEvent::ModelUsed { model: model.to_string() });
            }
            // Fim do turno: sem `Done`, o app acusa falha mesmo com a resposta certa.
            events.push(ProviderEvent::Done { ok: value["finishReason"].as_str() == Some("completed") });
            events
        }
        Some("error") => vec![ProviderEvent::Error {
            message: format!("cline: {}", value["message"].as_str().unwrap_or("erro sem mensagem")),
        }],
        _ => vec![],
    }
}

fn agent_event(event: &Value) -> Vec<ProviderEvent> {
    match (event["type"].as_str(), event["contentType"].as_str()) {
        // O texto chega em content_start; o content_end repete o texto inteiro, então não gera evento.
        (Some("content_start"), Some("text")) => event["text"]
            .as_str()
            .map(|text| vec![ProviderEvent::TextDelta { text: text.to_string() }])
            .unwrap_or_default(),
        (Some("content_start"), Some("tool")) => {
            let name = event["toolName"].as_str().unwrap_or_default();
            vec![ProviderEvent::ToolUse {
                id: event["toolCallId"].as_str().unwrap_or_default().to_string(),
                name: name.to_string(),
                detail: tool_detail(name, &event["input"]),
            }]
        }
        (Some("content_end"), Some("tool")) => {
            let outputs = event["output"].as_array().cloned().unwrap_or_default();
            vec![ProviderEvent::ToolResult {
                id: event["toolCallId"].as_str().unwrap_or_default().to_string(),
                ok: !outputs.is_empty() && outputs.iter().all(|o| o["success"].as_bool() == Some(true)),
                output: String::new(),
            }]
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Permission, ProviderKind, RunRequest};
    use super::*;

    #[test]
    fn prompt_goes_in_a_file_and_the_model_is_passed_with_m() {
        let args = args(Some("minimax/minimax-m3.1-flash"), Path::new("C:/tmp/prompt.txt"));
        assert_eq!(args[0], "--json");
        assert_eq!(&args[1..3], ["-m", "minimax/minimax-m3.1-flash"]);
        assert!(args.last().unwrap().contains("C:/tmp/prompt.txt"));
    }

    #[test]
    fn text_comes_from_content_start_and_the_end_repeat_is_ignored() {
        let start = r#"{"type":"agent_event","event":{"type":"content_start","contentType":"text","text":"ok","accumulated":"ok"}}"#;
        let end = r#"{"type":"agent_event","event":{"type":"content_end","contentType":"text","text":"ok"}}"#;
        assert_eq!(parse_line(start), vec![ProviderEvent::TextDelta { text: "ok".into() }]);
        assert!(parse_line(end).is_empty());
    }

    #[test]
    fn tool_call_and_result_are_linked_by_id() {
        let start = r#"{"type":"agent_event","event":{"type":"content_start","contentType":"tool","toolCallId":"call_1","toolName":"run_commands","input":{"commands":["Get-Date"]}}}"#;
        let end = r#"{"type":"agent_event","event":{"type":"content_end","contentType":"tool","toolCallId":"call_1","toolName":"run_commands","output":[{"query":"Get-Date","result":"ok","success":true}]}}"#;
        assert_eq!(
            parse_line(start),
            vec![ProviderEvent::ToolUse { id: "call_1".into(), name: "run_commands".into(), detail: "Get-Date".into() }]
        );
        assert_eq!(parse_line(end), vec![ProviderEvent::ToolResult { id: "call_1".into(), ok: true, output: String::new() }]);
    }

    #[test]
    fn the_run_result_says_which_model_answered() {
        let line = r#"{"type":"run_result","finishReason":"completed","text":"ok","model":{"id":"minimax/minimax-m3.1-flash","provider":"cline"}}"#;
        assert!(parse_line(line).contains(&ProviderEvent::ModelUsed { model: "minimax/minimax-m3.1-flash".into() }));
    }

    /// O `run_result` fecha o turno (capturado em todo turno): sem `Done`, o app acusa falha com saída 0.
    #[test]
    fn the_run_result_closes_the_turn() {
        let completed = r#"{"type":"run_result","finishReason":"completed","model":{"id":"minimax/minimax-m3.1-flash"}}"#;
        assert!(parse_line(completed).contains(&ProviderEvent::Done { ok: true }));
        let failed = r#"{"type":"run_result","finishReason":"error","model":{"id":"minimax/minimax-m3.1-flash"}}"#;
        assert!(parse_line(failed).contains(&ProviderEvent::Done { ok: false }));
    }

    #[test]
    fn cline_only_runs_with_full_access() {
        let mut request = RunRequest {
            provider: ProviderKind::Cline,
            prompt: "x".into(),
            session_id: None,
            cwd: ".".into(),
            permission: Permission::ReadOnly,
            browser: Default::default(),
            browser_mode: false,
            web_search: false,
            subagents: false,
            model: None,
            effort: None,
            profile: None,
            images: vec![],
            context_window: None,
            long_cache: false,
        };
        assert!(request.validate().is_err());
        request.permission = Permission::FullAccess;
        assert!(request.validate().is_ok());
    }

    #[test]
    fn hooks_and_bad_json_do_not_break_the_chat() {
        assert!(parse_line(r#"{"type":"hook_event","hookEventName":"agent_start"}"#).is_empty());
        assert!(matches!(parse_line("não é json")[0], ProviderEvent::Error { .. }));
    }

    #[test]
    fn login_is_read_from_the_account_folder_only_when_it_has_a_credential() {
        let root = std::env::temp_dir().join(format!("lunaharness-cline-login-{}", std::process::id()));
        let settings = root.join("data").join("settings");
        std::fs::create_dir_all(&settings).unwrap();
        let base = root.to_string_lossy().into_owned();

        assert!(!logged_in_at(Some(&base)));
        std::fs::write(
            settings.join("providers.json"),
            r#"{"lastUsedProvider":"openrouter","providers":{"openrouter":{"settings":{"auth":{"apiKey":"k"}}}}}"#,
        )
        .unwrap();
        assert!(logged_in_at(Some(&base)));

        let _ = std::fs::remove_dir_all(&root);
    }
}
