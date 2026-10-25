use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{cli_name, Permission, ProviderEvent, ProviderKind, RunRequest};

/// O Copilot não tem comando de status. O app manda uma pergunta curta e vê se a CLI responde com sucesso.
/// Cada verificação gasta uma requisição premium, então o resultado vale por 30 minutos (ou até um login novo).
const LOGIN_CHECK_TTL: Duration = Duration::from_secs(30 * 60);
static LOGIN_CHECK: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Argumentos da verificação: uma resposta sem ferramentas, sem perguntas e sem sessão na tela.
fn login_probe_args() -> Vec<String> {
    ["-p", "Responda apenas ok.", "-s", "--no-ask-user"].map(String::from).into()
}

/// Entrou na conta? Sem conta extra (env vazio), usa o resultado guardado enquanto ele é recente.
/// Com conta extra, roda a verificação na hora, na pasta da conta (`COPILOT_HOME`), sem guardar o resultado.
pub fn logged_in_with(env: &[(String, String)]) -> bool {
    if !env.is_empty() {
        return probe(env);
    }
    if let Ok(guard) = LOGIN_CHECK.lock() {
        if let Some((at, ok)) = *guard {
            if at.elapsed() < LOGIN_CHECK_TTL {
                return ok;
            }
        }
    }
    let ok = probe(&[]);
    if let Ok(mut guard) = LOGIN_CHECK.lock() {
        *guard = Some((Instant::now(), ok));
    }
    ok
}

/// Esquece o resultado guardado: depois de um login novo, a próxima verificação roda de verdade.
pub fn forget_login() {
    if let Ok(mut guard) = LOGIN_CHECK.lock() {
        *guard = None;
    }
}

/// Roda a verificação. Sem CLI, ou com erro, conta como não conectado e deixa o motivo no log (sem o texto da conta).
fn probe(env: &[(String, String)]) -> bool {
    let Some(program) = crate::session::resolve_program(cli_name(ProviderKind::Copilot)) else {
        eprintln!("Copilot: CLI não encontrada ao verificar o login.");
        return false;
    };
    let output = crate::session::hidden_command(&program)
        .args(login_probe_args())
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            eprintln!("Copilot: a verificação de login terminou com código {:?}; o app mostra como não conectado.", output.status.code());
            false
        }
        Err(e) => {
            eprintln!("Copilot: não foi possível rodar a verificação de login: {e}");
            false
        }
    }
}

/// `copilot --output-format json` com o prompt pelo stdin (testado na CLI 1.0.100). Sem `--no-ask-user` a CLI
/// pausa para pedir permissão e trava a execução sem terminal. A permissão vira lista de ferramentas liberadas:
/// `view` (leitura) e `apply_patch` (edição, nome real da ferramenta de escrita); o acesso total libera tudo.
pub fn args(request: &RunRequest) -> Vec<String> {
    let mut args: Vec<String> = ["--output-format", "json", "--no-ask-user"].map(String::from).into();
    match request.permission {
        Permission::ReadOnly => args.extend(["--allow-tool".to_string(), "view".to_string()]),
        Permission::AcceptEdits => args.extend(["--allow-tool", "view", "--allow-tool", "apply_patch"].map(String::from)),
        Permission::FullAccess => args.push("--allow-all-tools".into()),
    }
    if let Some(id) = &request.session_id {
        args.push(format!("--resume={id}"));
    }
    // "auto" é a opção "Automático" do seletor: sem `--model`, a CLI escolhe o modelo.
    if let Some(model) = request.model.as_deref().filter(|m| *m != "auto") {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    if let Some(effort) = &request.effort {
        args.extend(["--reasoning-effort".to_string(), effort.clone()]);
    }
    args
}

/// Detalhe curto da ferramenta: o caminho do arquivo ou o comando.
fn tool_detail(input: &Value) -> String {
    ["path", "command"]
        .iter()
        .find_map(|key| input[*key].as_str())
        .unwrap_or_default()
        .to_string()
}

/// Uma linha do `copilot --output-format json` (JSONL). Formato capturado com a CLI 1.0.100: o texto vem em
/// `assistant.message_delta`; `assistant.message` traz o texto inteiro e o modelo; ferramentas em `tool.execution_*`;
/// `result` no fim com o id da sessão (para `--resume`) e o código de saída.
/// Eventos de sessão, de MCP e de contagem não viram nada no chat.
pub fn parse_line(line: &str) -> Vec<ProviderEvent> {
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            return vec![ProviderEvent::Error {
                message: "copilot: linha de saída não é JSON válido".into(),
            }]
        }
    };
    let data = &value["data"];
    match value["type"].as_str() {
        Some("assistant.message_delta") => data["deltaContent"]
            .as_str()
            .map(|text| vec![ProviderEvent::TextDelta { text: text.to_string() }])
            .unwrap_or_default(),
        Some("assistant.message") => data["model"]
            .as_str()
            .map(|model| vec![ProviderEvent::ModelUsed { model: model.to_string() }])
            .unwrap_or_default(),
        Some("tool.execution_start") => vec![ProviderEvent::ToolUse {
            id: data["toolCallId"].as_str().unwrap_or_default().to_string(),
            name: data["toolName"].as_str().unwrap_or_default().to_string(),
            detail: tool_detail(&data["arguments"]),
        }],
        Some("tool.execution_complete") => vec![ProviderEvent::ToolResult {
            id: data["toolCallId"].as_str().unwrap_or_default().to_string(),
            ok: data["success"].as_bool() == Some(true),
            output: String::new(),
        }],
        Some("result") => {
            let mut events = Vec::new();
            if let Some(id) = value["sessionId"].as_str() {
                events.push(ProviderEvent::SessionStarted { session_id: id.to_string() });
            }
            let code = value["exitCode"].as_i64();
            // Esta linha é o fim do turno: sem `Done`, o app acusa falha mesmo com saída 0.
            events.push(ProviderEvent::Done { ok: matches!(code, Some(0) | None) });
            if let Some(code) = code.filter(|c| *c != 0) {
                events.push(ProviderEvent::Error {
                    message: format!("copilot: terminou com código {code}"),
                });
            }
            events
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::super::ProviderKind;
    use super::*;

    fn request(permission: Permission) -> RunRequest {
        RunRequest {
            provider: ProviderKind::Copilot,
            prompt: "x".into(),
            session_id: None,
            cwd: ".".into(),
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
    fn every_mode_asks_nothing_and_allows_only_its_tools() {
        let read = args(&request(Permission::ReadOnly));
        assert!(read.contains(&"--no-ask-user".to_string()));
        assert_eq!(read.iter().filter(|a| *a == "--allow-tool").count(), 1);
        assert!(!read.contains(&"apply_patch".to_string()));

        let edit = args(&request(Permission::AcceptEdits));
        assert!(edit.contains(&"apply_patch".to_string()));

        let full = args(&request(Permission::FullAccess));
        assert!(full.contains(&"--allow-all-tools".to_string()));
    }

    #[test]
    fn resume_model_and_effort_become_flags() {
        let mut req = request(Permission::AcceptEdits);
        req.session_id = Some("e24b7bcc".into());
        req.model = Some("gpt-5.6-luna".into());
        req.effort = Some("high".into());
        let built = args(&req);
        assert!(built.contains(&"--resume=e24b7bcc".to_string()));
        assert_eq!(&built[built.len() - 4..], ["--model", "gpt-5.6-luna", "--reasoning-effort", "high"]);
    }

    #[test]
    fn text_comes_from_deltas_and_the_full_message_only_names_the_model() {
        let delta = r#"{"type":"assistant.message_delta","data":{"messageId":"m","deltaContent":"ok"}}"#;
        let message = r#"{"type":"assistant.message","data":{"content":"ok","model":"gpt-5.6-luna","toolRequests":[]}}"#;
        assert_eq!(parse_line(delta), vec![ProviderEvent::TextDelta { text: "ok".into() }]);
        assert_eq!(parse_line(message), vec![ProviderEvent::ModelUsed { model: "gpt-5.6-luna".into() }]);
    }

    #[test]
    fn tool_call_and_result_are_linked_by_id() {
        let start = r#"{"type":"tool.execution_start","data":{"toolCallId":"call_1","toolName":"view","arguments":{"path":"C:\\a.txt"}}}"#;
        let end = r#"{"type":"tool.execution_complete","data":{"toolCallId":"call_1","success":true,"result":{"content":"x"}}}"#;
        assert_eq!(
            parse_line(start),
            vec![ProviderEvent::ToolUse { id: "call_1".into(), name: "view".into(), detail: "C:\\a.txt".into() }]
        );
        assert_eq!(parse_line(end), vec![ProviderEvent::ToolResult { id: "call_1".into(), ok: true, output: String::new() }]);
    }

    #[test]
    fn the_result_line_keeps_the_session_and_reports_a_failed_exit() {
        let ok = r#"{"type":"result","sessionId":"e24b7bcc","exitCode":0}"#;
        assert_eq!(
            parse_line(ok),
            vec![ProviderEvent::SessionStarted { session_id: "e24b7bcc".into() }, ProviderEvent::Done { ok: true }]
        );
        let failed = r#"{"type":"result","sessionId":"e24b7bcc","exitCode":1}"#;
        assert!(matches!(parse_line(failed).last(), Some(ProviderEvent::Error { .. })));
    }

    /// "Automático" é a opção que deixa a CLI escolher: não pode virar `--model auto`, que a CLI recusa.
    #[test]
    fn automatic_model_sends_no_model_flag() {
        let mut req = request(Permission::ReadOnly);
        req.model = Some("auto".into());
        assert!(!args(&req).contains(&"--model".to_string()));
    }

    /// A verificação de login é uma pergunta sem ferramentas e sem sessão na tela, e sem stdin.
    #[test]
    fn login_check_asks_one_short_question_without_tools() {
        assert_eq!(login_probe_args(), ["-p", "Responda apenas ok.", "-s", "--no-ask-user"]);
    }

    /// A linha `result` fecha o turno: sem `Done`, o app mostra "terminou sem concluir a resposta" mesmo com saída 0.
    #[test]
    fn result_line_closes_the_turn_so_a_clean_exit_is_not_an_error() {
        let ok = r#"{"type":"result","sessionId":"e24b7bcc","exitCode":0}"#;
        assert!(parse_line(ok).contains(&ProviderEvent::Done { ok: true }));
        let failed = r#"{"type":"result","sessionId":"e24b7bcc","exitCode":1}"#;
        assert!(parse_line(failed).contains(&ProviderEvent::Done { ok: false }));
    }

    #[test]
    fn session_and_mcp_noise_gives_no_events() {
        assert!(parse_line(r#"{"type":"session.mcp_server_status_changed","data":{}}"#).is_empty());
        assert!(parse_line(r#"{"type":"user.message","data":{"content":"oi"}}"#).is_empty());
    }
}
