//! Hermes (Nous Research), pelo `hermes chat --oneshot -Q --format stream-json`. Formato medido com a CLI 0.21.6
//! (captura em tests/fixtures/hermes_stream.jsonl): `system` (init), `tool_use`, `tool_result`, `text` e `result`.
//! Antes do JSON a CLI escreve um aviso de texto puro; as linhas que não são JSON são ignoradas.

use serde_json::Value;

use super::{Permission, ProviderEvent, RunRequest};

/// Modelo do pedido quando não há escolha: o Copilot, que responde nesta conta (o Nous Portal sem créditos recusa os pagos).
pub const DEFAULT_MODEL: &str = "copilot:gpt-4.1";

/// Modelo do seletor no formato `provedor:modelo` (ex.: `nous:z-ai/glm-5.2`). Sem provedor conhecido, vale o modelo como ele é no Copilot.
pub fn split_model(model: &str) -> (&str, &str) {
    match model.split_once(':') {
        Some((provider @ ("nous" | "copilot"), name)) => (provider, name),
        _ => ("copilot", model),
    }
}

/// Argumentos da execução. O prompt vai pelo stdin (`--query-file -`), como nos outros CLIs.
/// Só o acesso total chega aqui (ver `RunRequest::validate`), por isso `--yolo` vai sempre.
pub fn args(request: &RunRequest) -> Vec<String> {
    let (provider, model) = split_model(request.model.as_deref().unwrap_or(DEFAULT_MODEL));
    let mut args: Vec<String> = [
        "chat",
        "--query-file",
        "-",
        "--oneshot",
        "-Q",
        "--format",
        "stream-json",
        "--provider",
        provider,
        "-m",
    ]
    .map(String::from)
    .into();
    args.push(model.into());
    if let Some(id) = &request.session_id {
        args.extend(["--resume".to_string(), id.clone()]);
    }
    args.push("--yolo".into());
    args
}

/// Só acesso total: o Hermes não tem modo de leitura com pedido de aprovação neste app.
pub fn check_permission(permission: Permission) -> Result<(), String> {
    if permission == Permission::FullAccess {
        Ok(())
    } else {
        Err("O Hermes só roda com acesso total neste app.".into())
    }
}

/// Detalhe curto da ferramenta: o comando ou o caminho.
fn tool_detail(input: &Value) -> String {
    ["command", "path"].iter().find_map(|key| input[*key].as_str()).unwrap_or_default().to_string()
}

/// Uma linha do stream. Sem id no formato: o chamado e o resultado usam o mesmo id, e o chat liga o resultado
/// ao chamado mais recente (ferramentas rodam em sequência).
pub fn parse_line(line: &str) -> Vec<ProviderEvent> {
    // Aviso de texto puro antes do JSON (ex.: "conjuntos de ferramentas desconhecidos"): não é resposta.
    if !line.trim_start().starts_with('{') {
        return vec![];
    }
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            return vec![ProviderEvent::Error {
                message: "hermes: linha de saída não é JSON válido".into(),
            }]
        }
    };
    match value["type"].as_str() {
        Some("system") => {
            let mut events = Vec::new();
            if let Some(id) = value["session_id"].as_str() {
                events.push(ProviderEvent::SessionStarted { session_id: id.to_string() });
            }
            if let Some(model) = value["model"].as_str() {
                events.push(ProviderEvent::ModelUsed { model: model.to_string() });
            }
            events
        }
        Some("text") => value["text"]
            .as_str()
            .map(|text| vec![ProviderEvent::TextDelta { text: text.to_string() }])
            .unwrap_or_default(),
        Some("tool_use") => vec![ProviderEvent::ToolUse {
            id: "hermes".into(),
            name: value["name"].as_str().unwrap_or_default().to_string(),
            detail: tool_detail(&value["input"]),
        }],
        Some("tool_result") => vec![ProviderEvent::ToolResult {
            id: "hermes".into(),
            ok: !value["is_error"].as_bool().unwrap_or(false),
            output: value["output"].as_str().unwrap_or_default().to_string(),
        }],
        // O texto final já veio nos eventos `text`; aqui só vale se a execução falhou.
        Some("result") => {
            let failed = value["exit_code"].as_i64().unwrap_or(0) != 0 || value["error"].is_string();
            if !failed {
                return vec![ProviderEvent::Done { ok: true }];
            }
            let message = value["error"].as_str().unwrap_or("hermes: a resposta falhou sem mensagem");
            vec![ProviderEvent::Error { message: message.to_string() }, ProviderEvent::Done { ok: false }]
        }
        _ => vec![],
    }
}

/// Conectado = alguma credencial guardada pelo Hermes. `hermes auth list` lista cada provedor com a contagem,
/// como `nous (1 credentials):`; só conta quando a contagem é maior que zero.
pub fn has_credentials(output: &str) -> bool {
    output.lines().any(|line| {
        line.split_once(" credentials)")
            .and_then(|(before, _)| before.rsplit('(').next())
            .and_then(|count| count.trim().parse::<u32>().ok())
            .is_some_and(|count| count > 0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTURED: &str = include_str!("../../tests/fixtures/hermes_stream.jsonl");

    fn request(model: Option<&str>, session: Option<&str>) -> RunRequest {
        RunRequest {
            provider: crate::providers::ProviderKind::Hermes,
            prompt: "x".into(),
            session_id: session.map(String::from),
            cwd: std::path::PathBuf::from("C:\\projeto"),
            permission: Permission::FullAccess,
            model: model.map(String::from),
            effort: None,
            profile: None,
            context_window: None,
            browser: Default::default(),
            browser_mode: false,
            subagents: false,
            long_cache: false,
            images: Vec::new(),
        }
    }

    #[test]
    fn captured_run_gives_session_model_tool_and_text_then_done() {
        let events: Vec<ProviderEvent> = CAPTURED.lines().flat_map(parse_line).collect();

        assert!(events.contains(&ProviderEvent::SessionStarted { session_id: "20261020_153432_b96244".into() }));
        assert!(events.contains(&ProviderEvent::ModelUsed { model: "gpt-4.1".into() }));
        assert!(events.contains(&ProviderEvent::ToolUse { id: "hermes".into(), name: "terminal".into(), detail: "echo oi".into() }));
        assert!(events.contains(&ProviderEvent::ToolResult { id: "hermes".into(), ok: true, output: "{\"output\": \"oi\", \"exit_code\": 0, \"error\": null}".into() }));
        assert!(events.iter().any(|e| matches!(e, ProviderEvent::TextDelta { text } if text.contains("pr"))));
        assert_eq!(events.last(), Some(&ProviderEvent::Done { ok: true }));
    }

    #[test]
    fn warning_before_the_json_is_ignored() {
        assert!(parse_line("Aviso: conjuntos de ferramentas desconhecidos: sn").is_empty());
    }

    /// Erro real (modelo recusado pelo Nous, capturado): falha do turno com a mensagem da CLI.
    #[test]
    fn failed_result_fails_the_turn_with_the_reason() {
        let line = r#"{"type": "result", "session_id": "s", "exit_code": 1, "text": "bla", "error": "HTTP 404: This model has been retired."}"#;

        assert_eq!(
            parse_line(line),
            vec![
                ProviderEvent::Error { message: "HTTP 404: This model has been retired.".into() },
                ProviderEvent::Done { ok: false },
            ]
        );
    }

    #[test]
    fn args_use_stdin_query_stream_json_and_the_default_model() {
        assert_eq!(
            args(&request(None, None)),
            ["chat", "--query-file", "-", "--oneshot", "-Q", "--format", "stream-json", "--provider", "copilot", "-m", "gpt-4.1", "--yolo"]
        );
    }

    #[test]
    fn resume_passes_the_session_and_the_chosen_model() {
        let args = args(&request(Some("gpt-4.1"), Some("20261020_1")));

        let model = args.iter().position(|a| a == "-m").unwrap();
        assert_eq!(args[model + 1], "gpt-4.1");
        let resume = args.iter().position(|a| a == "--resume").unwrap();
        assert_eq!(args[resume + 1], "20261020_1");
    }

    /// O modelo do Nous Portal leva o provedor dele; o padrão continua no Copilot.
    #[test]
    fn the_provider_comes_from_the_model_prefix() {
        let args = args(&request(Some("nous:z-ai/glm-5.2"), None));

        let provider = args.iter().position(|a| a == "--provider").unwrap();
        assert_eq!(args[provider + 1], "nous");
        let model = args.iter().position(|a| a == "-m").unwrap();
        assert_eq!(args[model + 1], "z-ai/glm-5.2");
        assert_eq!(split_model("gpt-4.1"), ("copilot", "gpt-4.1"));
    }

    #[test]
    fn only_full_access_is_accepted() {
        assert!(check_permission(Permission::FullAccess).is_ok());
        assert!(check_permission(Permission::ReadOnly).is_err());
        assert!(check_permission(Permission::AcceptEdits).is_err());
    }

    #[test]
    fn a_provider_with_a_credential_counts_as_connected() {
        let output = "copilot (1 credentials):\n  #1  gh auth token  api_key\n\nnous (1 credentials):\n  #1  oauth\n";
        assert!(has_credentials(output));
    }

    #[test]
    fn empty_listings_are_not_connected() {
        assert!(!has_credentials(""));
        assert!(!has_credentials("nous (0 credentials):\nopenai-codex (0 credentials):\n"));
    }
}
