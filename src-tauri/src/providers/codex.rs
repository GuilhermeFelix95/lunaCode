use serde_json::Value;

use super::{ProviderEvent, TodoItem};

use super::{identity_for, AgentBrowser, Permission, RunRequest};

/// Overrides de config que sobem o servidor `luna-browser` no Codex. Caminho, endereço e token entram como
/// strings JSON, que são strings TOML válidas (as barras do Windows ficam escapadas).
fn browser_server_flags(server: &crate::browser::McpServer) -> Vec<String> {
    let quoted = |text: &str| serde_json::to_string(text).unwrap_or_default();
    vec![
        "-c".into(),
        format!("mcp_servers.luna-browser.command={}", quoted(&server.command)),
        "-c".into(),
        "mcp_servers.luna-browser.args=[\"browser-mcp\"]".into(),
        "-c".into(),
        format!(
            "mcp_servers.luna-browser.env={{LUNA_BROWSER_URL={},LUNA_BROWSER_TOKEN={}}}",
            quoted(&server.url),
            quoted(&server.token)
        ),
    ]
}

/// `codex exec --json -` lê o prompt do stdin e emite JSONL.
/// `exec resume` não aceita `-s`, então a retomada passa o sandbox por `-c sandbox_mode`.
pub fn args(request: &RunRequest) -> Vec<String> {
    let sandbox = sandbox_mode(request.permission);
    let mut args: Vec<String> = vec!["exec".into()];
    // Uma flag `-i` por imagem, sempre antes de `--json`: assim o `-` do stdin nunca vira valor de imagem.
    let images: Vec<String> = request
        .images
        .iter()
        .flat_map(|p| ["-i".to_string(), p.display().to_string()])
        .collect();

    // `-s` só existe em `exec` novo; a retomada recebe o mesmo modo por `-c sandbox_mode`.
    match &request.session_id {
        None => {
            args.extend(images.iter().cloned());
            args.extend(["--json", "--skip-git-repo-check", "-s", sandbox].map(String::from));
        }
        Some(_) => args.extend(["-c".to_string(), format!("sandbox_mode=\"{sandbox}\"")]),
    }

    if let Some(model) = &request.model {
        args.extend(["-m".to_string(), model.clone()]);
    }
    if let Some(effort) = &request.effort {
        args.extend(["-c".to_string(), format!("model_reasoning_effort=\"{effort}\"")]);
    }
    // Identidade da Luna como instrução do desenvolvedor, nos dois casos (novo e retomada).
    args.extend(["-c".to_string(), format!("developer_instructions=\"{}\"", identity_for(request))]);
    // Servidor de etapas (lunatasks): sempre, pelo mesmo caminho do Playwright. Sem ele, o checklist depende só do Codex.
    let tasks = crate::tasks_mcp::command();
    args.extend(["-c".to_string(), format!("mcp_servers.{}.command=\"{}\"", crate::tasks_mcp::SERVER, tasks[0])]);
    args.extend(["-c".to_string(), format!("mcp_servers.{}.args=[\"{}\"]", crate::tasks_mcp::SERVER, tasks[1])]);
    // Navegador do Playwright só quando escolhido e em uso (modo navegador ou painel aberto, como no
    // Claude): o Codex sobe o servidor por override de config (sem mexer no config do usuário).
    // Fora de uso, as ferramentas ficavam no prefixo de todo turno sem ninguém pedir.
    if super::browser_in_use(request) && request.browser == AgentBrowser::Playwright {
        let (command, server_args) = crate::playwright::server_command();
        let list: Vec<String> = server_args.iter().map(|a| format!("\"{a}\"")).collect();
        args.extend(["-c".to_string(), format!("mcp_servers.playwright.command=\"{command}\"")]);
        args.extend(["-c".to_string(), format!("mcp_servers.playwright.args=[{}]", list.join(","))]);
    }
    // Navegador embutido: o mesmo servidor que o Claude recebe, pela mesma regra de uso.
    if super::browser_in_use(request) && request.browser == AgentBrowser::Builtin {
        if let Some(server) = crate::browser::mcp_server() {
            args.extend(browser_server_flags(server));
        }
    }
    // Deixa o agente ler o guia de plugins, e só essa pasta.
    if let Some(dir) = crate::plugins::guide::dir() {
        args.extend(["--add-dir".to_string(), dir.display().to_string()]);
    }
    if let Some(dir) = crate::plugins::guide::drafts_dir() {
        args.extend(["--add-dir".to_string(), dir.display().to_string()]);
    }

    match &request.session_id {
        None => args.push("-".into()),
        Some(id) => {
            args.extend(["resume".to_string(), id.clone()]);
            args.extend(images.iter().cloned());
            args.extend(["--json".to_string(), "--skip-git-repo-check".into(), "-".into()]);
        }
    }
    args
}

fn sandbox_mode(permission: Permission) -> &'static str {
    match permission {
        // `codex exec` não tem canal de aprovação: `Pedir permissão` cai para só leitura, como decidido no plano.
        Permission::ReadOnly | Permission::AcceptEdits => "read-only",
        Permission::FullAccess => "danger-full-access",
    }
}

#[cfg(test)]
mod sandbox_tests {
    use super::*;

    #[test]
    fn asking_for_permission_falls_back_to_read_only_in_codex() {
        assert_eq!(sandbox_mode(Permission::AcceptEdits), "read-only");
        assert_eq!(sandbox_mode(Permission::ReadOnly), "read-only");
        assert_eq!(sandbox_mode(Permission::FullAccess), "danger-full-access");
    }
}

/// Etapas de um `todo_list` do Codex: `completed` vira concluída; sem ele, a etapa está pendente.
/// Formato da documentação do Codex; ainda não confirmado com uma execução real (login pendente nesta máquina).
fn codex_todos(item: &Value) -> Vec<ProviderEvent> {
    let items = item["items"]
        .as_array()
        .map(|todos| {
            todos
                .iter()
                .map(|todo| TodoItem {
                    content: todo["text"].as_str().unwrap_or_default().to_string(),
                    status: if todo["completed"].as_bool() == Some(true) { "completed" } else { "pending" }.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    vec![ProviderEvent::Todos { items }]
}

/// Etapas de uma chamada `set_steps` do servidor lunatasks. None para as outras chamadas de ferramenta.
/// Formato do item (mcp_tool_call: server, tool, arguments) da documentação do Codex; sem captura real ainda (login pendente nesta máquina).
fn steps_call(item: &Value) -> Option<Vec<ProviderEvent>> {
    if item["server"].as_str() != Some(crate::tasks_mcp::SERVER) || item["tool"].as_str() != Some(crate::tasks_mcp::TOOL) {
        return None;
    }
    crate::tasks_mcp::parse_steps(&item["arguments"]).ok().map(|items| vec![ProviderEvent::Todos { items }])
}

/// Eventos `error` de nível superior são avisos de reconexão do próprio Codex,
/// então são ignorados. Falha real chega em `turn.failed`.
pub fn parse_line(line: &str) -> Vec<ProviderEvent> {
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            return vec![ProviderEvent::Error {
                message: "codex: linha de saída não é JSON válido".into(),
            }]
        }
    };

    // Lista de etapas (todo_list) chega em item.started, item.updated e item.completed: a lista inteira vale a cada evento.
    let is_item_event = value.get("type").and_then(Value::as_str).is_some_and(|t| t.starts_with("item."));
    if is_item_event && value["item"]["type"].as_str() == Some("todo_list") {
        return codex_todos(&value["item"]);
    }
    // Chamada de set_steps do servidor lunatasks: a lista vem nos argumentos, e o Progresso acompanha a cada chamada.
    if is_item_event && value["item"]["type"].as_str() == Some("mcp_tool_call") {
        if let Some(events) = steps_call(&value["item"]) {
            return events;
        }
    }

    match value.get("type").and_then(Value::as_str) {
        Some("thread.started") => match value.get("thread_id").and_then(Value::as_str) {
            Some(id) => vec![ProviderEvent::SessionStarted {
                session_id: id.to_string(),
            }],
            None => vec![],
        },
        Some("item.completed") => {
            let item = &value["item"];
            match item.get("type").and_then(Value::as_str) {
                Some("agent_message") => item
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|text| {
                        vec![ProviderEvent::TextDelta {
                            text: text.to_string(),
                        }]
                    })
                    .unwrap_or_default(),
                Some("command_execution") => {
                    // Sem exit_code (versões antigas), vale o status: "failed" é erro, qualquer outro é ok.
                    let ok = item["exit_code"].as_i64().map_or(item["status"].as_str() != Some("failed"), |code| code == 0);
                    vec![
                        ProviderEvent::ToolUse {
                            id: item["id"].as_str().unwrap_or_default().to_string(),
                            name: "Bash".into(),
                            detail: item["command"].as_str().unwrap_or_default().to_string(),
                        },
                        ProviderEvent::ToolResult {
                            id: item["id"].as_str().unwrap_or_default().to_string(),
                            ok,
                            output: item["aggregated_output"].as_str().unwrap_or_default().to_string(),
                        },
                    ]
                }
                Some("file_change") => {
                    let paths: Vec<&str> = item["changes"]
                        .as_array()
                        .map(|changes| {
                            changes
                                .iter()
                                .filter_map(|c| c.get("path").and_then(Value::as_str))
                                .collect()
                        })
                        .unwrap_or_default();
                    vec![ProviderEvent::ToolUse {
                        id: item["id"].as_str().unwrap_or_default().to_string(),
                        name: "Editar".into(),
                        detail: paths.join(", "),
                    }]
                }
                _ => vec![],
            }
        }
        Some("turn.completed") => {
            // O Codex informa a entrada total e quantos tokens vieram do cache. Não informa gravação no cache.
            let usage = &value["usage"];
            let mut events = Vec::new();
            if let Some(input) = usage["input_tokens"].as_u64() {
                let read = usage["cached_input_tokens"].as_u64();
                let fresh = input.saturating_sub(read.unwrap_or(0));
                events.push(ProviderEvent::CacheUsage { fresh: Some(fresh), read, write: None });
            }
            // O Codex informa a saída só no fim do turno: é uma chamada só, então a velocidade sai do valor final.
            if let Some(tokens) = usage["output_tokens"].as_u64() {
                events.push(ProviderEvent::OutputTokens { tokens });
            }
            events.push(ProviderEvent::Done { ok: true });
            events
        }
        Some("turn.failed") => {
            let message = value["error"]["message"]
                .as_str()
                .unwrap_or("codex: turno falhou sem mensagem")
                .to_string();
            vec![
                ProviderEvent::Error { message },
                ProviderEvent::Done { ok: false },
            ]
        }
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Formato da documentação do Codex (ainda sem execução real): a lista inteira vira o checklist.
    #[test]
    fn todo_list_becomes_the_checklist_on_every_update() {
        let events = parse_line(
            r#"{"type":"item.updated","item":{"type":"todo_list","items":[{"text":"Criar arquivo","completed":true},{"text":"Testar","completed":false}]}}"#,
        );

        assert_eq!(
            events,
            vec![ProviderEvent::Todos {
                items: vec![
                    TodoItem { content: "Criar arquivo".into(), status: "completed".into() },
                    TodoItem { content: "Testar".into(), status: "pending".into() },
                ],
            }]
        );
    }

    const FAILED_TURN: &str = include_str!("../../tests/fixtures/codex_stream.jsonl");

    #[test]
    fn captured_unauthenticated_run_reports_failure_once() {
        let events: Vec<ProviderEvent> = FAILED_TURN.lines().flat_map(parse_line).collect();

        assert!(matches!(events.first(), Some(ProviderEvent::SessionStarted { .. })));
        // Apenas o turn.failed vira erro; os avisos "Reconnecting..." são ignorados.
        let errors = events
            .iter()
            .filter(|e| matches!(e, ProviderEvent::Error { .. }))
            .count();
        assert_eq!(errors, 1);
        assert_eq!(events.last(), Some(&ProviderEvent::Done { ok: false }));
    }

    #[test]
    fn agent_message_becomes_text() {
        let line = r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"ok"}}"#;
        assert_eq!(
            parse_line(line),
            vec![ProviderEvent::TextDelta { text: "ok".into() }]
        );
    }

    /// Formato documentado do `codex exec --json`. Ainda não validado com captura real,
    /// porque o Codex desta máquina não está logado; validar no smoke test.
    #[test]
    fn command_execution_becomes_bash_tool_use() {
        let line = r#"{"type":"item.completed","item":{"id":"item_2","type":"command_execution","command":"dir","status":"completed"}}"#;
        assert_eq!(
            parse_line(line),
            vec![
                ProviderEvent::ToolUse {
                    id: "item_2".into(),
                    name: "Bash".into(),
                    detail: "dir".into()
                },
                ProviderEvent::ToolResult { id: "item_2".into(), ok: true, output: String::new() },
            ]
        );
    }

    /// Comando que falha: `exit_code` diferente de zero vira resultado com ok = false e a saída no fim.
    #[test]
    fn a_failed_command_reports_exit_code_and_output() {
        let line = r#"{"type":"item.completed","item":{"id":"item_4","type":"command_execution","command":"npm test","aggregated_output":"1 teste falhou","exit_code":1,"status":"failed"}}"#;
        assert!(parse_line(line).contains(&ProviderEvent::ToolResult {
            id: "item_4".into(),
            ok: false,
            output: "1 teste falhou".into(),
        }));
    }

    #[test]
    fn file_change_lists_changed_paths() {
        let line = r#"{"type":"item.completed","item":{"id":"item_3","type":"file_change","changes":[{"path":"a.ts","kind":"update"},{"path":"b.ts","kind":"add"}]}}"#;
        assert_eq!(
            parse_line(line),
            vec![ProviderEvent::ToolUse {
                id: "item_3".into(),
                name: "Editar".into(),
                detail: "a.ts, b.ts".into()
            }]
        );
    }

    #[test]
    fn turn_completed_reports_cached_input_before_done() {
        assert_eq!(
            parse_line(r#"{"type":"turn.completed","usage":{"input_tokens":1000,"cached_input_tokens":800,"output_tokens":30}}"#),
            vec![
                ProviderEvent::CacheUsage { fresh: Some(200), read: Some(800), write: None },
                ProviderEvent::OutputTokens { tokens: 30 },
                ProviderEvent::Done { ok: true },
            ]
        );
    }

    #[test]
    fn turn_completed_is_success() {
        assert_eq!(
            parse_line(r#"{"type":"turn.completed","usage":{}}"#),
            vec![ProviderEvent::Done { ok: true }]
        );
    }

    /// A identidade vai como instrução do desenvolvedor, antes do `-` do stdin, em pedido novo e em retomada.
    #[test]
    fn identity_is_a_developer_instruction_before_the_stdin_marker() {
        for session in [None, Some("th-9".to_string())] {
            let request = RunRequest {
                provider: crate::providers::ProviderKind::Codex,
                prompt: "x".into(),
                session_id: session,
                cwd: std::path::PathBuf::from("C:\\projeto"),
                permission: Permission::ReadOnly,
                model: None,
                effort: None,
                profile: None,
                context_window: None,
                browser: Default::default(),
                browser_mode: false,
                web_search: false,
                subagents: false,
                long_cache: false,
                images: Vec::new(),
            };

            let expected = format!("developer_instructions=\"{}\"", identity_for(&request));
            let args = args(&request);

            let at = args.iter().position(|a| *a == expected).expect("falta a identidade");
            assert_eq!(args[at - 1], "-c");
            assert_eq!(args.last().map(String::as_str), Some("-"));
        }
    }

    /// Cada imagem ganha a sua flag \`-i\`, antes de \`--json\`; o \`-\` do stdin fica por último.
    #[test]
    fn each_image_gets_its_own_flag_before_the_stdin_marker() {
        let request = RunRequest {
            provider: crate::providers::ProviderKind::Codex,
            prompt: "x".into(),
            session_id: None,
            cwd: std::path::PathBuf::from("C:\\projeto"),
            permission: Permission::ReadOnly,
            model: None,
            effort: None,
            profile: None,
            context_window: None,
            browser: Default::default(),
            browser_mode: false,
            web_search: false,
            subagents: false,
            long_cache: false,
            images: vec![
                std::path::PathBuf::from("C:\\a\\1.png"),
                std::path::PathBuf::from("C:\\a\\2.jpg"),
            ],
        };
        let flags = args(&request);
        let first = flags.iter().position(|a| a == "-i").unwrap();
        assert_eq!(flags[first + 1], "C:\\a\\1.png");
        assert_eq!(flags.iter().filter(|a| *a == "-i").count(), 2);
        assert!(first < flags.iter().position(|a| a == "--json").unwrap());
        assert_eq!(flags.last().unwrap(), "-");
    }

    /// Com o Playwright escolhido, o Codex sobe o servidor por override de config, antes do `-` do stdin.
    #[test]
    fn tasks_server_is_always_added_and_its_calls_become_steps() {
        let request = RunRequest {
            provider: crate::providers::ProviderKind::Codex,
            prompt: "x".into(),
            session_id: None,
            cwd: std::path::PathBuf::from("C:\\projeto"),
            permission: Permission::ReadOnly,
            model: None,
            effort: None,
            profile: None,
            context_window: None,
            browser: Default::default(),
            browser_mode: false,
            web_search: false,
            subagents: false,
            long_cache: false,
            images: Vec::new(),
        };
        let flags = args(&request);
        assert!(flags.iter().any(|a| a.starts_with("mcp_servers.lunatasks.command=")));
        assert!(flags.iter().any(|a| a == "mcp_servers.lunatasks.args=[\"tasks-mcp\"]"));

        let line = r#"{"type":"item.started","item":{"id":"i1","type":"mcp_tool_call","server":"lunatasks","tool":"set_steps","arguments":{"steps":[{"content":"Criar hello","status":"in_progress"}]}}}"#;
        assert_eq!(parse_line(line), vec![ProviderEvent::Todos { items: vec![TodoItem { content: "Criar hello".into(), status: "in_progress".into() }] }]);
    }

    #[test]
    fn the_builtin_browser_reaches_codex_as_a_config_override() {
        let server = crate::browser::McpServer {
            command: r"C:\Luna\lunacode.exe".into(),
            url: "http://127.0.0.1:9/tool".into(),
            token: "segredo".into(),
        };
        let flags = browser_server_flags(&server);
        // O caminho do Windows sai com as barras escapadas, como string TOML.
        assert!(flags.contains(&r#"mcp_servers.luna-browser.command="C:\\Luna\\lunacode.exe""#.to_string()));
        assert!(flags.contains(&"mcp_servers.luna-browser.args=[\"browser-mcp\"]".to_string()));
        assert!(flags.iter().any(|a| a.starts_with("mcp_servers.luna-browser.env={") && a.contains(r#"LUNA_BROWSER_TOKEN="segredo""#)));
        assert!(flags.iter().all(|a| a == "-c" || a.starts_with("mcp_servers.luna-browser.")));
    }

    #[test]
    fn playwright_is_added_only_when_chosen_and_in_use() {
        let mut request = RunRequest {
            provider: crate::providers::ProviderKind::Codex,
            prompt: "x".into(),
            session_id: None,
            cwd: std::path::PathBuf::from("C:\\projeto"),
            permission: Permission::ReadOnly,
            model: None,
            effort: None,
            profile: None,
            context_window: None,
            long_cache: false,
            browser: crate::providers::AgentBrowser::Builtin,
            browser_mode: false,
            web_search: false,
            subagents: false,
            images: Vec::new(),
        };
        assert!(!args(&request).iter().any(|a| a.contains("mcp_servers.playwright")));

        request.browser = crate::providers::AgentBrowser::Playwright;
        // Escolhido, mas fora de uso (sem modo navegador nem painel aberto): o servidor fica de fora.
        assert!(!args(&request).iter().any(|a| a.contains("mcp_servers.playwright")));

        request.browser_mode = true;
        let flags = args(&request);
        assert!(flags.iter().any(|a| a.starts_with("mcp_servers.playwright.command=")));
        assert!(flags.iter().any(|a| a.starts_with("mcp_servers.playwright.args=") && a.contains("@playwright/mcp")));
        assert_eq!(flags.last().unwrap(), "-");
    }
}
