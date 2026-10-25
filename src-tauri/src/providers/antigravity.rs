use serde_json::Value;

use super::{Permission, ProviderEvent, RunRequest};

/// `agy -p=<prompt> --output-format stream-json` (testado com o agy de 2026-10-20). O prompt vai colado ao `-p`:
/// com `-p` solto, o agy lê o próximo flag como prompt, e o stdin não é lido. O modo de permissão vai em `--mode`.
pub fn args(request: &RunRequest) -> Vec<String> {
    let mut args = vec![format!("-p={}", request.prompt), "--output-format".into(), "stream-json".into()];
    match request.permission {
        // `agy` não tem canal de aprovação em modo `-p`: `Pedir permissão` cai para só leitura, como decidido no plano.
        Permission::ReadOnly | Permission::AcceptEdits => args.extend(["--mode".to_string(), "plan".to_string()]),
        // Acesso total: o agy aprova tudo sem perguntar. Sem essa flag, ele nega comandos num processo sem terminal.
        Permission::FullAccess => args.push("--dangerously-skip-permissions".into()),
    }
    if let Some(id) = &request.session_id {
        args.extend(["--conversation".to_string(), id.clone()]);
    }
    if let Some(model) = &request.model {
        args.extend(["--model".to_string(), model.clone()]);
    }
    args
}

/// Nome da ferramenta como o chat mostra. `run_command` vira `Bash` para o `CommandTracker` acompanhar o resultado.
fn tool_name(name: &str) -> String {
    match name {
        "run_command" => "Bash",
        "read_file" => "Read",
        "write_file" => "Write",
        other => other,
    }
    .to_string()
}

/// Detalhe curto da ferramenta: o caminho do arquivo ou o comando. O agy usa nomes com maiúscula (`Path`, `CommandLine`).
fn tool_detail(parameters: &Value) -> String {
    ["Path", "CommandLine", "path", "command"]
        .iter()
        .find_map(|key| parameters[*key].as_str())
        .unwrap_or_default()
        .to_string()
}

/// Uma linha do `agy --output-format stream-json`. Formato capturado: `init` com o id da conversa; `step_update`
/// com um passo por vez (`agent_response` traz o texto em `text_delta`; `tool` traz nome, parâmetros e estado);
/// `result` no fim com o status e as ações que a CLI negou por não conseguir pedir aprovação.
/// Linhas que não são JSON (avisos da CLI no stdout) não viram nada no chat.
pub fn parse_line(line: &str) -> Vec<ProviderEvent> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    match value["event"].as_str() {
        Some("init") => value["conversation_id"]
            .as_str()
            .map(|id| vec![ProviderEvent::SessionStarted { session_id: id.to_string() }])
            .unwrap_or_default(),
        Some("step_update") => step(&value["step_update"]),
        Some("result") => result(&value["result"]),
        _ => vec![],
    }
}

fn step(update: &Value) -> Vec<ProviderEvent> {
    match update["step_type"].as_str() {
        Some("agent_response") => update["text_delta"]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(|text| vec![ProviderEvent::TextDelta { text: text.to_string() }])
            .unwrap_or_default(),
        // O estado ACTIVE traz o chamado; DONE e ERROR fecham o mesmo passo (`step_index`), que liga os dois.
        Some("tool") => {
            let id = update["step_index"].to_string();
            match update["state"].as_str() {
                Some("ACTIVE") => vec![ProviderEvent::ToolUse {
                    id,
                    name: tool_name(update["tool_name"].as_str().unwrap_or_default()),
                    detail: tool_detail(&update["tool_info"]["parameters"]),
                }],
                Some("DONE") => {
                    let output = update["tool_info"]["output"].as_str().unwrap_or_default().to_string();
                    vec![ProviderEvent::ToolResult { id, ok: true, output }]
                }
                Some("ERROR") => {
                    let message = update["tool_info"]["error"]["message"].as_str().unwrap_or_default().to_string();
                    vec![ProviderEvent::ToolResult { id, ok: false, output: message }]
                }
                _ => vec![],
            }
        }
        _ => vec![],
    }
}

fn result(result: &Value) -> Vec<ProviderEvent> {
    let mut events = Vec::new();
    let failed = result["status"].as_str() == Some("ERROR");
    if failed {
        let message = result["error"].as_str().unwrap_or("erro sem mensagem");
        events.push(ProviderEvent::Error { message: format!("antigravity: {message}") });
    }
    // Ação que a CLI não pôde aprovar sozinha: sem terminal, ela nega. O chat precisa dizer por quê.
    if let Some(denied) = result["denied_actions"].as_array().filter(|a| !a.is_empty()) {
        let names: Vec<&str> = denied.iter().filter_map(|a| a["display_name"].as_str()).collect();
        events.push(ProviderEvent::Error {
            message: format!(
                "antigravity: a CLI negou {} porque, sem terminal, não consegue pedir aprovação.",
                names.join(", ")
            ),
        });
    }
    // Esta linha é o fim do turno: sem `Done`, o app acusa falha mesmo com status SUCCESS.
    events.push(ProviderEvent::Done { ok: !failed });
    events
}

/// Lista do `agy models`: uma linha por modelo, `id<TAB>nome`. As outras linhas (ex.: "Fetching...") são ignoradas.
pub fn parse_catalog(raw: &str) -> Vec<(String, String)> {
    raw.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(id, label)| (id.trim().to_string(), label.trim().to_string()))
        .filter(|(id, _)| !id.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::ProviderKind;
    use super::*;
    use std::path::PathBuf;

    fn request(permission: Permission) -> RunRequest {
        RunRequest {
            provider: ProviderKind::Antigravity,
            prompt: "oi, tudo bem?".into(),
            session_id: None,
            cwd: PathBuf::from("."),
            permission,
            browser: Default::default(),
            browser_mode: false,
            subagents: false,
            model: None,
            effort: None,
            profile: None,
            images: vec![],
            context_window: None,
            long_cache: false,
        }
    }

    #[test]
    fn prompt_is_attached_to_p_and_permission_is_a_mode() {
        let plan = args(&request(Permission::ReadOnly));
        assert_eq!(plan, ["-p=oi, tudo bem?", "--output-format", "stream-json", "--mode", "plan"]);

        // `Pedir permissão` sem canal de aprovação vira só leitura.
        let edits = args(&request(Permission::AcceptEdits));
        assert_eq!(&edits[3..], ["--mode", "plan"]);
    }

    #[test]
    fn resume_and_model_become_flags() {
        let mut req = request(Permission::AcceptEdits);
        req.session_id = Some("4e339a54-9bc3".into());
        req.model = Some("gemini-3.8-flash-low".into());
        let built = args(&req);
        assert_eq!(&built[5..], ["--conversation", "4e339a54-9bc3", "--model", "gemini-3.8-flash-low"]);
    }

    #[test]
    fn init_gives_the_conversation_id_used_to_resume() {
        let line = r#"{"event":"init","conversation_id":"4e339a54","init":{"cwd":"C:\\p","tools":["read_file"]}}"#;
        assert_eq!(parse_line(line), vec![ProviderEvent::SessionStarted { session_id: "4e339a54".into() }]);
    }

    #[test]
    fn agent_text_comes_from_text_delta() {
        let line = r#"{"event":"step_update","step_update":{"conversation_id":"c","step_index":1,"state":"ACTIVE","step_type":"agent_response","text_delta":"ok"}}"#;
        assert_eq!(parse_line(line), vec![ProviderEvent::TextDelta { text: "ok".into() }]);
    }

    #[test]
    fn command_call_is_named_bash_and_its_error_keeps_the_message() {
        let start = r#"{"event":"step_update","step_update":{"step_index":2,"state":"ACTIVE","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","parameters":{"CommandLine":"echo ola"}}}}"#;
        assert_eq!(
            parse_line(start),
            vec![ProviderEvent::ToolUse { id: "2".into(), name: "Bash".into(), detail: "echo ola".into() }]
        );

        let failed = r#"{"event":"step_update","step_update":{"step_index":2,"state":"ERROR","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","error":{"type":"TOOL_ERROR","message":"user denied permission"}}}}"#;
        assert_eq!(
            parse_line(failed),
            vec![ProviderEvent::ToolResult { id: "2".into(), ok: false, output: "user denied permission".into() }]
        );
    }

    #[test]
    fn command_output_comes_with_the_done_state() {
        let done = r#"{"event":"step_update","step_update":{"step_index":2,"state":"DONE","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","parameters":{"CommandLine":"echo ola-total"},"output":"Command exited with code 0.\n\nCommand output:\nola-total\r\n"}}}"#;
        assert_eq!(
            parse_line(done),
            vec![ProviderEvent::ToolResult { id: "2".into(), ok: true, output: "Command exited with code 0.\n\nCommand output:\nola-total\r\n".into() }]
        );
    }

    #[test]
    fn read_and_write_keep_the_file_path() {
        let read = r#"{"event":"step_update","step_update":{"step_index":2,"state":"ACTIVE","step_type":"tool","tool_name":"read_file","tool_info":{"parameters":{"Path":"C:\\a.txt"}}}}"#;
        assert_eq!(
            parse_line(read),
            vec![ProviderEvent::ToolUse { id: "2".into(), name: "Read".into(), detail: "C:\\a.txt".into() }]
        );
    }

    #[test]
    fn denied_actions_and_failed_status_become_errors() {
        let denied = r#"{"event":"result","result":{"status":"SUCCESS","response":"","denied_actions":[{"action":"command","display_name":"RunCommand"}]}}"#;
        let events = parse_line(denied);
        assert!(events.iter().any(|e| matches!(e, ProviderEvent::Error { message } if message.contains("RunCommand"))));

        let failed = r#"{"event":"result","result":{"status":"ERROR","error":"stream input message is missing the \"event\" field"}}"#;
        assert!(parse_line(failed).iter().any(|e| matches!(e, ProviderEvent::Error { .. })));
    }

    /// A linha `result` fecha o turno: sem `Done`, o app acusa falha mesmo com status SUCCESS.
    #[test]
    fn result_line_closes_the_turn_so_a_clean_run_is_not_an_error() {
        let ok = r#"{"event":"result","result":{"status":"SUCCESS","response":"ok\n","denied_actions":[]}}"#;
        assert_eq!(parse_line(ok), vec![ProviderEvent::Done { ok: true }]);
        let failed = r#"{"event":"result","result":{"status":"ERROR","error":"falhou"}}"#;
        assert!(parse_line(failed).contains(&ProviderEvent::Done { ok: false }));
    }

    #[test]
    fn plain_lines_give_no_events() {
        assert!(parse_line("jetski: no output produced").is_empty());
    }

    #[test]
    fn full_access_skips_permissions_and_is_allowed() {
        let full = args(&request(Permission::FullAccess));
        assert_eq!(&full[3..], ["--dangerously-skip-permissions"]);
        assert!(request(Permission::FullAccess).validate().is_ok());
    }

    #[test]
    fn catalog_reads_id_and_name_per_line_and_skips_the_rest() {
        let raw = "Fetching available models...\ngemini-3.8-flash-low\tGemini 3.8 Flash (Low)\nclaude-sonnet-4-6\tClaude Sonnet 4.6 (Thinking)\n";
        assert_eq!(
            parse_catalog(raw),
            vec![
                ("gemini-3.8-flash-low".to_string(), "Gemini 3.8 Flash (Low)".to_string()),
                ("claude-sonnet-4-6".to_string(), "Claude Sonnet 4.6 (Thinking)".to_string()),
            ]
        );
    }
}
