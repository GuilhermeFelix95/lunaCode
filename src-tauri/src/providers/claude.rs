use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use super::{DiffKind, DiffLine, FileDiff, ProviderEvent, TodoItem};

use super::{identity_for, image_media_type, AgentBrowser, LimitWindow, Permission, ProviderKind, RunRequest};
use crate::permission_prompt::{self, PROMPT_TOOL};

/// Sem isso, o Claude Code oferece as ferramentas de tarefa só para alguns modelos; com esta variável elas ficam ligadas.
/// Confirmado numa execução real: com ela, o agente cria e atualiza tarefas (TaskCreate / TaskUpdate).
pub fn task_env() -> (String, String) {
    ("CLAUDE_CODE_ENABLE_TODO_TOOLS".to_string(), "true".to_string())
}

/// Em tarefas com etapas, o agente registra cada etapa e atualiza o estado ao avançar: é o que alimenta a aba Progresso.
const TASK_INSTRUCTION: &str = "Em tarefas com mais de uma etapa, registre as etapas com a ferramenta de tarefas (TaskCreate) antes de começar e atualize o estado de cada uma (TaskUpdate) ao avançar.";

/// `claude -p` lê o prompt do stdin e emite JSON por linha.
/// `--include-partial-messages` manda os deltas de texto conforme chegam.
pub fn args(request: &RunRequest) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-mode",
        permission_mode(request.permission),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    // Pedir permissão: o CLI pergunta ao app antes de ações sensíveis, pelo servidor de permissões.
    // Sem esse servidor no ar, o turno segue sem pop-up e o CLI nega o que precisaria de confirmação.
    let prompt_config = (request.permission == Permission::AcceptEdits).then(permission_prompt::mcp_config).flatten();
    let mut mcp_configs: Vec<String> = prompt_config.map(|config| config.display().to_string()).into_iter().collect();
    if prompt_config.is_some() {
        args.extend(["--permission-prompt-tool".to_string(), PROMPT_TOOL.to_string()]);
    }
    if !request.images.is_empty() {
        args.extend(["--input-format".to_string(), "stream-json".to_string()]);
    }

    // Acrescenta a identidade ao prompt padrão, que diz que o agente é o Claude Code.
    // Uma linha só: quebra de linha num argumento quebra o .cmd do Windows.
    // Com subagente escolhido, a Luna manda cada etapa para ele e acompanha o resultado.
    let subagent = request.subagents.then(crate::orchestrator::agents_path).flatten();
    let delegation = if subagent.is_some() { " Ao executar um plano, delegue cada etapa ao subagente luna-etapa e acompanhe o resultado." } else { "" };
    args.extend(["--append-system-prompt".to_string(), format!("{} {TASK_INSTRUCTION}{delegation}", identity_for(request))]);
    if let Some(path) = subagent {
        args.extend(["--agents".to_string(), path.display().to_string()]);
    }
    // Deixa o agente ler o guia de plugins, e só essa pasta. Sempre seguido de uma flag, pois --add-dir aceita vários valores.
    if let Some(dir) = crate::plugins::guide::dir() {
        args.extend(["--add-dir".to_string(), dir.display().to_string()]);
    }
    // Só o navegador escolhido em Configurações chega ao agente, e só quando ele está em uso:
    // modo navegador ligado ou o painel já aberto nesta execução. Liberado sem perguntar, como o embutido.
    // Fora de uso, as 9 ferramentas ficavam no prefixo de todo turno sem ninguém pedir.
    let server = match request.browser {
        AgentBrowser::Builtin => crate::browser::mcp_config(),
        AgentBrowser::Playwright => crate::playwright::mcp_config(),
    };
    let browser_config = browser_server(super::browser_in_use(request), request.browser, server);
    // Um só `--mcp-config` com todas as configurações: um segundo flag poderia substituir o primeiro.
    if let Some((config, allowed)) = browser_config {
        mcp_configs.push(config.display().to_string());
        args.extend(["--allowedTools".to_string(), allowed.to_string()]);
    }
    if !mcp_configs.is_empty() {
        args.push("--mcp-config".to_string());
        args.extend(mcp_configs);
    }
    // Onde a Luna cria rascunhos de plugin para a pessoa instalar pelo cartão no chat.
    if let Some(dir) = crate::plugins::guide::drafts_dir() {
        args.extend(["--add-dir".to_string(), dir.display().to_string()]);
    }
    if let Some(model) = &request.model {
        args.extend(["--model".to_string(), model.clone()]);
    }
    if let Some(effort) = &request.effort {
        args.extend(["--effort".to_string(), effort.clone()]);
    }
    if let Some(id) = &request.session_id {
        args.push("--resume".into());
        args.push(id.clone());
    }
    args
}

/// Servidor do navegador que entra no turno, ou `None` quando ele não está em uso (nem quando o app
/// não conseguiu gravar a configuração). É o que economiza os tokens das 9 ferramentas paradas.
fn browser_server(in_use: bool, browser: AgentBrowser, config: Option<&Path>) -> Option<(PathBuf, &'static str)> {
    if !in_use {
        return None;
    }
    let path = config?.to_path_buf();
    Some(match browser {
        AgentBrowser::Builtin => (path, "mcp__luna-browser"),
        AgentBrowser::Playwright => (path, crate::playwright::CLAUDE_ALLOWED),
    })
}

/// Execução avulsa (segundo plano): mesmo motor Claude, mesma pasta e permissão do turno,
/// sem retomar a sessão da conversa nem usar o subagente das etapas do plano.
pub fn background_request(
    prompt: String,
    cwd: std::path::PathBuf,
    permission: Permission,
    model: Option<String>,
) -> RunRequest {
    RunRequest {
        provider: ProviderKind::Claude,
        prompt,
        session_id: None,
        cwd,
        permission,
        browser: Default::default(),
        browser_mode: false,
        subagents: false,
        model,
        effort: None,
        profile: None,
        context_window: None,
        long_cache: false,
        images: Vec::new(),
    }
}

/// Argumentos de uma execução avulsa: os do turno menos `--resume` e `--agents`.
pub fn background_args(request: &RunRequest) -> Vec<String> {
    let mut kept = Vec::new();
    let mut skip_next = false;
    for arg in args(request) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--resume" || arg == "--agents" {
            skip_next = true;
            continue;
        }
        kept.push(arg);
    }
    kept
}

/// Entrada do CLI pelo stdin. Sem imagens é o prompt puro. Com imagens, uma linha stream-json:
/// o texto e um bloco de imagem em base64 para cada arquivo.
pub fn input(request: &RunRequest) -> Result<String, String> {
    if request.images.is_empty() {
        return Ok(request.prompt.clone());
    }
    let mut content = vec![json!({ "type": "text", "text": request.prompt })];
    for path in &request.images {
        let media_type = image_media_type(path).ok_or("formato de imagem não suportado")?;
        let bytes = std::fs::read(path)
            .map_err(|e| format!("não foi possível ler a imagem {}: {e}", path.display()))?;
        content.push(json!({
            "type": "image",
            "source": { "type": "base64", "media_type": media_type, "data": STANDARD.encode(bytes) }
        }));
    }
    let line = json!({ "type": "user", "message": { "role": "user", "content": content } });
    Ok(format!("{line}\n"))
}

fn permission_mode(permission: Permission) -> &'static str {
    match permission {
        Permission::ReadOnly => "plan",
        // Pedir permissão: o modo padrão do CLI pergunta antes de ações sensíveis; o pedido passa pelo servidor do app.
        Permission::AcceptEdits => "default",
        Permission::FullAccess => "bypassPermissions",
    }
}

pub fn parse_line(line: &str) -> Vec<ProviderEvent> {
    let value: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => {
            return vec![ProviderEvent::Error {
                message: "claude: linha de saída não é JSON válido".into(),
            }]
        }
    };

    match value.get("type").and_then(Value::as_str) {
        Some("system") if value.get("subtype").and_then(Value::as_str) == Some("init") => {
            match value.get("session_id").and_then(Value::as_str) {
                Some(id) => vec![ProviderEvent::SessionStarted {
                    session_id: id.to_string(),
                }],
                None => vec![],
            }
        }
        // Texto chega em deltas (`--include-partial-messages`). O texto da mensagem `assistant`
        // completa é ignorado para não duplicar a resposta.
        Some("stream_event") => {
            // Ao fim de cada chamada ao modelo, `message_delta` traz quantos tokens de saída ela gerou.
            if value["event"]["type"].as_str() == Some("message_delta") {
                return value["event"]["usage"]["output_tokens"].as_u64().map(|tokens| ProviderEvent::OutputTokens { tokens }).into_iter().collect();
            }
            let delta = &value["event"]["delta"];
            if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
                return vec![];
            }
            match delta.get("text").and_then(Value::as_str) {
                Some(text) if !text.is_empty() => vec![ProviderEvent::TextDelta {
                    text: text.to_string(),
                }],
                _ => vec![],
            }
        }
        Some("assistant") => {
            // Cada mensagem traz o modelo que respondeu: é o que mostra o papel usado no chat.
            let mut events: Vec<ProviderEvent> = value["message"]["model"]
                .as_str()
                .map(|model| ProviderEvent::ModelUsed { model: model.to_string() })
                .into_iter()
                .collect();
            events.extend(
                value["message"]["content"]
                    .as_array()
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
                            .flat_map(tool_use_events)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            );
            events
        }
        // O resultado de TaskCreate traz o id da tarefa; ele liga o tool_use_id ao id que o TaskUpdate usa.
        Some("user") => {
            let mut events = task_assigned_events(&value);
            events.extend(tool_result_events(&value));
            events
        }
        Some("rate_limit_event") => rate_limit_events(&value),
        Some("result") => {
            let is_error = value.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            let mut events = Vec::new();
            if is_error {
                let message = value
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or("claude: execução terminou com erro")
                    .to_string();
                events.push(ProviderEvent::Error { message });
            }
            events.extend(context_event(&value));
            events.extend(cache_usage_event(&value));
            // Com API, a CLI informa o custo do turno. Com assinatura, o campo não vem.
            if let Some(usd) = value.get("total_cost_usd").and_then(Value::as_f64) {
                events.push(ProviderEvent::Cost { usd });
            }
            events.push(ProviderEvent::Done { ok: !is_error });
            events
        }
        _ => vec![],
    }
}

/// `rate_limit_event` traz as duas janelas em `unifiedWindows`. Cada uma vira um evento.
fn rate_limit_events(value: &Value) -> Vec<ProviderEvent> {
    let windows = &value["rate_limit_info"]["unifiedWindows"];
    [("five_hour", LimitWindow::FiveHour), ("seven_day", LimitWindow::SevenDay)]
        .into_iter()
        .filter_map(|(key, window)| {
            let info = windows.get(key)?;
            Some(ProviderEvent::RateLimit {
                window,
                utilization: info["utilization"].as_f64()?,
                resets_at: info["resetsAt"].as_i64()?,
            })
        })
        .collect()
}

/// No fim do turno, `modelUsage` dá o tamanho da janela e `usage` diz quantos tokens entraram.
/// Os tokens que contam para o contexto são os da última chamada ao modelo.
fn context_event(value: &Value) -> Option<ProviderEvent> {
    // A janela pode faltar (API de terceiros): o evento sai mesmo assim, com window_size vazio.
    let window_size = value["modelUsage"]
        .as_object()
        .and_then(|models| models.values().next())
        .and_then(|model| model["contextWindow"].as_u64());

    let usage = &value["usage"];
    if usage.is_null() {
        return None;
    }
    let last = usage["iterations"].as_array().and_then(|list| list.last()).unwrap_or(usage);
    let tokens = ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]
        .iter()
        .map(|key| last[*key].as_u64().unwrap_or(0))
        .sum();

    Some(ProviderEvent::Context { tokens, window_size })
}

/// Tokens de entrada do turno, do `usage` do resultado: novos, lidos do cache e gravados no cache.
fn cache_usage_event(value: &Value) -> Option<ProviderEvent> {
    let usage = value.get("usage").filter(|u| !u.is_null())?;
    let field = |key: &str| usage.get(key).and_then(Value::as_u64);
    Some(ProviderEvent::CacheUsage {
        fresh: field("input_tokens"),
        read: field("cache_read_input_tokens"),
        write: field("cache_creation_input_tokens"),
    })
}

/// Linha de ferramenta e, quando a ferramenta mexe no checklist, o evento de tarefa correspondente.
fn tool_use_events(block: &Value) -> Vec<ProviderEvent> {
    let name = block["name"].as_str().unwrap_or("ferramenta").to_string();
    let input = &block["input"];
    let id = block["id"].as_str().unwrap_or_default().to_string();
    let mut events = vec![ProviderEvent::ToolUse { id: id.clone(), name: name.clone(), detail: tool_detail(input) }];
    if let Some(diff) = file_diff(&name, input) {
        events.push(ProviderEvent::ToolDiff { id, diff });
    }
    match name.as_str() {
        "TodoWrite" => events.extend(todos_event(input)),
        "TaskCreate" => events.push(ProviderEvent::TaskCreated {
            key: block["id"].as_str().unwrap_or_default().to_string(),
            subject: input["subject"].as_str().unwrap_or_default().to_string(),
        }),
        "TaskUpdate" => events.push(ProviderEvent::TaskUpdated {
            id: input["taskId"].as_str().unwrap_or_default().to_string(),
            status: input["status"].as_str().map(str::to_string),
            subject: input["subject"].as_str().map(str::to_string),
        }),
        _ => {}
    }
    events
}

/// Resultado de cada ferramenta. `is_error` diz se falhou; o texto vira a saída do chamado.
fn tool_result_events(value: &Value) -> Vec<ProviderEvent> {
    value["message"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        .filter_map(|b| {
            let id = b["tool_use_id"].as_str()?.to_string();
            let ok = !b.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            Some(ProviderEvent::ToolResult { id, ok, output: result_text(&b["content"]) })
        })
        .collect()
}

/// O conteúdo do resultado é texto ou uma lista de blocos de texto.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// Resultado de TaskCreate: `tool_use_result.task.id` é o id da tarefa e `tool_use_id` liga ao TaskCreate.
fn task_assigned_events(value: &Value) -> Vec<ProviderEvent> {
    let Some(id) = value["tool_use_result"]["task"]["id"].as_str() else {
        return vec![];
    };
    value["message"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        .filter_map(|b| b["tool_use_id"].as_str())
        .map(|key| ProviderEvent::TaskAssigned { key: key.to_string(), id: id.to_string() })
        .collect()
}

/// Lista de etapas da ferramenta TodoWrite. Etapa sem texto ou sem status vira "" ou "pending".
fn todos_event(input: &Value) -> Option<ProviderEvent> {
    let items = input["todos"]
        .as_array()?
        .iter()
        .map(|todo| TodoItem {
            content: todo["content"].as_str().unwrap_or_default().to_string(),
            status: todo["status"].as_str().unwrap_or("pending").to_string(),
        })
        .collect();
    Some(ProviderEvent::Todos { items })
}

/// Resumo curto do que a ferramenta fez, para a linha de ferramenta no chat.
fn tool_detail(input: &Value) -> String {
    ["command", "file_path", "path", "pattern", "url", "description"]
        .iter()
        .find_map(|key| input.get(key).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

/// Limite de linhas guardadas no diff. As contagens continuam exatas além dele.
const MAX_DIFF_LINES: usize = 400;

/// Diff de Edit (old_string → new_string), MultiEdit (cada item de `edits`) e Write (todo o conteúdo como linhas novas).
fn file_diff(name: &str, input: &Value) -> Option<FileDiff> {
    let text = |value: &Value, key: &str| value[key].as_str().unwrap_or_default().to_string();
    let edits: Vec<(String, String)> = match name {
        "Edit" => vec![(text(input, "old_string"), text(input, "new_string"))],
        "MultiEdit" => input["edits"].as_array()?.iter().map(|edit| (text(edit, "old_string"), text(edit, "new_string"))).collect(),
        "Write" => vec![(String::new(), text(input, "content"))],
        _ => return None,
    };
    let mut diff = FileDiff { added: 0, removed: 0, lines: Vec::new() };
    for (old, new) in edits {
        for (kind, body) in [(DiffKind::Remove, old), (DiffKind::Add, new)] {
            for line in body.lines() {
                match kind {
                    DiffKind::Add => diff.added += 1,
                    DiffKind::Remove => diff.removed += 1,
                }
                if diff.lines.len() < MAX_DIFF_LINES {
                    diff.lines.push(DiffLine { kind, text: line.to_string() });
                }
            }
        }
    }
    Some(diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Edit, MultiEdit e Write mandam o diff do arquivo com as linhas removidas e adicionadas contadas.
    #[test]
    fn file_edits_report_their_diff() {
        let diff_of = |events: &[ProviderEvent]| {
            events.iter().find_map(|event| match event {
                ProviderEvent::ToolDiff { diff, .. } => Some((diff.added, diff.removed)),
                _ => None,
            })
        };
        let edit = tool_use_events(&json!({ "id": "e1", "name": "Edit", "input": { "file_path": "a.ts", "old_string": "x\ny", "new_string": "x\nz\nw" } }));
        assert_eq!(diff_of(&edit), Some((3, 2)));
        let write = tool_use_events(&json!({ "id": "w1", "name": "Write", "input": { "file_path": "b.ts", "content": "a\nb" } }));
        assert_eq!(diff_of(&write), Some((2, 0)));
        let multi = tool_use_events(&json!({ "id": "m1", "name": "MultiEdit", "input": { "file_path": "c.ts", "edits": [
            { "old_string": "p", "new_string": "q" },
            { "old_string": "", "new_string": "r" }
        ] } }));
        assert_eq!(diff_of(&multi), Some((2, 1)));
        let bash = tool_use_events(&json!({ "id": "b1", "name": "Bash", "input": { "command": "dir" } }));
        assert_eq!(diff_of(&bash), None);
    }

    /// Diff grande: as contagens são exatas, mas só as primeiras linhas vão para o chat.
    #[test]
    fn long_writes_keep_the_count_but_cut_the_lines() {
        let content = (0..500).map(|i| format!("linha {i}")).collect::<Vec<_>>().join("\n");
        let events = tool_use_events(&json!({ "id": "w2", "name": "Write", "input": { "file_path": "d.ts", "content": content } }));
        let diff = events.iter().find_map(|event| match event {
            ProviderEvent::ToolDiff { diff, .. } => Some(diff),
            _ => None,
        });
        let diff = diff.expect("Write gera diff");
        assert_eq!((diff.added, diff.lines.len()), (500, MAX_DIFF_LINES));
    }

    /// A entrada do turno é separada em novos, lidos e gravados no cache.
    #[test]
    fn cache_usage_splits_the_turn_input() {
        let value = json!({ "usage": { "input_tokens": 12, "cache_read_input_tokens": 900, "cache_creation_input_tokens": 40 } });
        assert_eq!(
            cache_usage_event(&value),
            Some(ProviderEvent::CacheUsage { fresh: Some(12), read: Some(900), write: Some(40) })
        );
    }

    /// Campo que a CLI não manda fica sem valor, e não vira zero.
    #[test]
    fn cache_usage_missing_fields_stay_none() {
        let value = json!({ "usage": { "input_tokens": 5 } });
        assert_eq!(
            cache_usage_event(&value),
            Some(ProviderEvent::CacheUsage { fresh: Some(5), read: None, write: None })
        );
    }
    use crate::providers::{ProviderKind, LUNA_IDENTITY};

    /// Cada `message_delta` do stream vira a contagem de saída daquela chamada ao modelo.
    #[test]
    fn message_delta_reports_the_output_tokens_of_that_call() {
        let line = r#"{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"input_tokens":2,"output_tokens":180}}}"#;
        assert_eq!(parse_line(line), vec![ProviderEvent::OutputTokens { tokens: 180 }]);
    }

    /// Sem o servidor de permissões no ar, `Pedir permissão` não liga o pop-up: o modo padrão segue, sem a ferramenta.
    #[test]
    fn asking_for_permission_without_the_server_sends_no_prompt_tool() {
        let request = RunRequest {
            provider: ProviderKind::Claude,
            prompt: "x".into(),
            session_id: None,
            cwd: std::path::PathBuf::from("C:\\projeto"),
            permission: Permission::AcceptEdits,
            model: None,
            effort: None,
            profile: None,
            context_window: None,
            browser: Default::default(),
            browser_mode: false,
            subagents: false,
            long_cache: false,
            images: Vec::new(),
        };
        let flags = args(&request);
        assert!(flags.windows(2).any(|window| window == ["--permission-mode", "default"]), "{flags:?}");
        assert!(!flags.iter().any(|arg| arg == "--permission-prompt-tool"), "sem servidor, sem ferramenta: {flags:?}");
    }

    /// A identidade vai junto de cada pedido, acrescentada ao prompt padrão (nunca substitui).
    #[test]
    fn identity_is_appended_to_the_default_prompt_on_every_run() {
        let request = RunRequest {
            provider: crate::providers::ProviderKind::Claude,
            prompt: "x".into(),
            session_id: Some("sess-1".into()),
            cwd: std::path::PathBuf::from("C:\\projeto"),
            permission: Permission::AcceptEdits,
            model: Some("deepseek-flash".into()),
            effort: None,
            profile: Some("p1".into()),
            context_window: None,
            browser: Default::default(),
            browser_mode: false,
            subagents: false,
            long_cache: false,
            images: Vec::new(),
        };

        let args = args(&request);

        assert!(args.windows(2).any(|w| w[0] == "--append-system-prompt" && w[1].starts_with(LUNA_IDENTITY) && w[1].contains("TaskCreate")));
        assert!(!args.iter().any(|a| a == "--system-prompt"));
    }

    #[test]
    fn browser_mode_appends_the_browser_identity_to_the_claude_prompt() {
        let request = RunRequest {
            provider: crate::providers::ProviderKind::Claude,
            prompt: "x".into(),
            session_id: None,
            cwd: std::path::PathBuf::from("C:\\projeto"),
            permission: Permission::ReadOnly,
            model: None,
            effort: None,
            profile: None,
            context_window: None,
            browser: Default::default(),
            browser_mode: true,
            subagents: false,
            long_cache: false,
            images: Vec::new(),
        };

        let args = args(&request);

        assert!(args.windows(2).any(|w| w[0] == "--append-system-prompt" && w[1].starts_with(crate::providers::BROWSER_IDENTITY)));
        assert!(!args.iter().any(|a| a.contains("api.registerCommand")));
    }

    /// As ferramentas do navegador entram no turno só quando ele está em uso: fora disso, o servidor
    /// nem aparece, e é isso que segura os tokens das 9 ferramentas paradas no prefixo.
    #[test]
    fn browser_server_is_loaded_only_when_the_browser_is_in_use() {
        let config = Some(Path::new("C:\\dados\\browser-mcp.json"));

        assert!(browser_server(false, AgentBrowser::Builtin, config).is_none());
        assert!(browser_server(false, AgentBrowser::Playwright, config).is_none());

        let (path, allowed) = browser_server(true, AgentBrowser::Builtin, config).unwrap();
        assert_eq!(allowed, "mcp__luna-browser");
        assert_eq!(path, PathBuf::from("C:\\dados\\browser-mcp.json"));

        let (_, allowed) = browser_server(true, AgentBrowser::Playwright, config).unwrap();
        assert_eq!(allowed, crate::playwright::CLAUDE_ALLOWED);

        // Sem configuração gravada não há servidor para passar.
        assert!(browser_server(true, AgentBrowser::Builtin, None).is_none());
    }

    const PARTIAL: &str = include_str!("../../tests/fixtures/claude_partial.jsonl");
    const TOOL: &str = include_str!("../../tests/fixtures/claude_tool.jsonl");
    const USAGE: &str = include_str!("../../tests/fixtures/claude_usage.jsonl");

    fn events_of(fixture: &str) -> Vec<ProviderEvent> {
        fixture.lines().flat_map(parse_line).collect()
    }

    fn text_of(events: &[ProviderEvent]) -> String {
        events
            .iter()
            .filter_map(|e| match e {
                ProviderEvent::TextDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn partial_capture_streams_text_and_finishes_ok() {
        let events = events_of(PARTIAL);

        assert!(matches!(
            events.first(),
            Some(ProviderEvent::SessionStarted { session_id }) if session_id == "a35297ea-77e7-4e26-8fda-ed4a29b80390"
        ));
        assert_eq!(text_of(&events), "ok");
        assert_eq!(events.last(), Some(&ProviderEvent::Done { ok: true }));
    }

    /// A mensagem `assistant` completa repete o texto que já chegou em deltas.
    /// Se o parser lesse os dois, a resposta apareceria duplicada.
    #[test]
    fn tool_capture_does_not_duplicate_text_and_reports_tool_use() {
        let events = events_of(TOOL);

        assert_eq!(text_of(&events), "feito");
        assert!(events.iter().any(|e| matches!(e,
            ProviderEvent::ToolUse { name, detail, .. } if name == "Bash" && detail == "echo lunaharness")));
        assert_eq!(events.last(), Some(&ProviderEvent::Done { ok: true }));
    }

    #[test]
    fn invalid_json_becomes_readable_error() {
        let events = parse_line("isto não é json");
        assert!(matches!(&events[0], ProviderEvent::Error { message } if message.contains("não é JSON")));
    }

    #[test]
    fn error_result_emits_error_then_failed_done() {
        let line = r#"{"type":"result","is_error":true,"result":"limite excedido"}"#;
        assert_eq!(
            parse_line(line),
            vec![
                ProviderEvent::Error {
                    message: "limite excedido".into()
                },
                ProviderEvent::Done { ok: false },
            ]
        );
    }

    /// Captura real: uma execução curta mostra as janelas de 5 horas e semana e o contexto do turno.
    #[test]
    fn real_capture_reports_both_limit_windows_and_context() {
        let events = events_of(USAGE);

        let limit = |want: LimitWindow| {
            events.iter().find_map(|e| match e {
                ProviderEvent::RateLimit { window, utilization, resets_at } if *window == want => {
                    Some((*utilization, *resets_at))
                }
                _ => None,
            })
        };
        let (five, five_reset) = limit(LimitWindow::FiveHour).expect("janela de 5 horas");
        let (week, week_reset) = limit(LimitWindow::SevenDay).expect("janela semanal");

        assert!((five - 0.45).abs() < 1e-9);
        assert_eq!(five_reset, 1792266000);
        assert!((week - 0.35).abs() < 1e-9);
        assert_eq!(week_reset, 1792512000);

        // input 2 + cache de leitura 26559 + cache de criação 17499.
        assert!(events.contains(&ProviderEvent::Context {
            tokens: 44060,
            window_size: Some(1_000_000),
        }));
    }

    /// No Windows, `claude` é um .cmd: quebra de linha ou aspas em argumento faz o cmd recusar a chamada
    /// ("batch file arguments are invalid"). Nenhum argumento pode ter esses caracteres.
    #[test]
    fn no_argument_has_line_breaks_or_quotes_that_break_the_cmd_shim() {
        let request = RunRequest {
            provider: ProviderKind::Claude,
            prompt: "oi".into(),
            session_id: None,
            cwd: ".".into(),
            permission: Permission::AcceptEdits,
            model: None,
            effort: None,
            profile: None,
            context_window: None,
            browser: Default::default(),
            browser_mode: false,
            subagents: false,
            long_cache: false,
            images: vec![],
        };

        for arg in args(&request) {
            assert!(!arg.contains(['\n', '\r', '"', '%']), "argumento inválido para o .cmd: {arg:?}");
        }
    }

    /// O Claude Code atual cria tarefas com TaskCreate e atualiza com TaskUpdate (formato medido numa execução real).
    #[test]
    fn task_create_and_update_become_task_events() {
        let create = parse_line(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"TaskCreate","input":{"subject":"Criar hello.txt","description":"d","activeForm":"Criando"}}]}}"#,
        );
        let update = parse_line(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_2","name":"TaskUpdate","input":{"taskId":"1","status":"completed"}}]}}"#,
        );

        assert!(create.contains(&ProviderEvent::TaskCreated { key: "toolu_1".into(), subject: "Criar hello.txt".into() }));
        assert!(update.contains(&ProviderEvent::TaskUpdated { id: "1".into(), status: Some("completed".into()), subject: None }));
    }

    /// Captura real do Claude Code (`claude -p` com `exit 3`): o resultado de erro vem com `is_error` e o texto.
    #[test]
    fn a_failed_bash_result_keeps_its_id_and_output() {
        let events = parse_line(
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"Exit code 3","is_error":true,"tool_use_id":"toolu_01NNztgG18jxGpgVmkZ6DYbw"}]},"parent_tool_use_id":null,"tool_use_result":"Error: Exit code 3"}"#,
        );

        assert_eq!(
            events,
            vec![ProviderEvent::ToolResult {
                id: "toolu_01NNztgG18jxGpgVmkZ6DYbw".into(),
                ok: false,
                output: "Exit code 3".into(),
            }]
        );
    }

    /// O id da tarefa só volta no resultado da ferramenta (mensagem `user`), ligado ao tool_use_id.
    #[test]
    fn the_task_id_comes_back_in_the_tool_result() {
        let events = parse_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"Task #1 created"}]},"tool_use_result":{"task":{"id":"1","subject":"Criar hello.txt"}}}"#,
        );

        assert!(events.contains(&ProviderEvent::TaskAssigned { key: "toolu_1".into(), id: "1".into() }));
    }

    /// Cada mensagem do assistente diz qual modelo respondeu: é a prova do papel usado.
    #[test]
    fn assistant_message_says_which_model_answered() {
        let events = parse_line(r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","content":[{"type":"text","text":"oi"}]}}"#);
        assert_eq!(events, vec![ProviderEvent::ModelUsed { model: "claude-haiku-5-5".into() }]);
    }

    /// TodoWrite vira o checklist da aba Progresso, na ordem que o agente escreveu.
    #[test]
    fn todo_write_becomes_an_ordered_checklist_and_keeps_the_tool_line() {
        let events = parse_line(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"TodoWrite","input":{"todos":[{"content":"Criar tela","status":"completed","activeForm":"Criando tela"},{"content":"Ligar a API","status":"in_progress"}]}}]}}"#,
        );

        assert_eq!(events[0], ProviderEvent::ToolUse { id: String::new(), name: "TodoWrite".into(), detail: String::new() });
        assert_eq!(
            events[1],
            ProviderEvent::Todos {
                items: vec![
                    TodoItem { content: "Criar tela".into(), status: "completed".into() },
                    TodoItem { content: "Ligar a API".into(), status: "in_progress".into() },
                ],
            }
        );
    }

    /// Endpoints de API (OpenRouter) não trazem a janela no resultado: os tokens ainda valem para o medidor.
    #[test]
    fn context_is_reported_even_when_the_window_size_is_unknown() {
        let value = serde_json::json!({
            "modelUsage": { "nvidia/nemotron-3.5-super-vl": {} },
            "usage": { "input_tokens": 1200, "cache_read_input_tokens": 300, "cache_creation_input_tokens": 0 }
        });

        assert_eq!(
            context_event(&value),
            Some(ProviderEvent::Context { tokens: 1500, window_size: None })
        );
    }

    #[test]
    fn unknown_types_are_ignored() {
        assert!(parse_line(r#"{"type":"tool_use_summary"}"#).is_empty());
    }

    #[test]
    fn images_switch_input_to_stream_json_with_one_block_per_image() {
        let dir = std::env::temp_dir().join(format!("lunaharness-claude-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("foto.png");
        std::fs::write(&path, [0x89u8, b'P', b'N', b'G']).unwrap();
        let request = RunRequest {
            provider: crate::providers::ProviderKind::Claude,
            prompt: "o que tem na imagem?".into(),
            session_id: None,
            cwd: std::path::PathBuf::from("C:\\projeto"),
            permission: Permission::ReadOnly,
            model: None,
            effort: None,
            profile: None,
            context_window: None,
            browser: Default::default(),
            browser_mode: false,
            subagents: false,
            long_cache: false,
            images: vec![path.clone()],
        };

        let flags = args(&request);
        assert!(flags.windows(2).any(|w| w == ["--input-format", "stream-json"]));

        let line = input(&request).unwrap();
        let value: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        let content = &value["message"]["content"];
        assert_eq!(content[0]["text"], "o que tem na imagem?");
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["media_type"], "image/png");
        let data = STANDARD.decode(content[1]["source"]["data"].as_str().unwrap()).unwrap();
        assert_eq!(data, vec![0x89u8, b'P', b'N', b'G']);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_images_the_prompt_goes_to_stdin_as_it_is() {
        let request = RunRequest {
            provider: crate::providers::ProviderKind::Claude,
            prompt: "oi".into(),
            session_id: None,
            cwd: std::path::PathBuf::from("C:\\projeto"),
            permission: Permission::ReadOnly,
            model: None,
            effort: None,
            profile: None,
            context_window: None,
            browser: Default::default(),
            browser_mode: false,
            subagents: false,
            long_cache: false,
            images: Vec::new(),
        };
        assert_eq!(input(&request).unwrap(), "oi");
        assert!(!args(&request).iter().any(|a| a == "--input-format"));
    }

    /// A tarefa em segundo plano usa a pasta da conversa e a permissão do turno, sem retomar a sessão
    /// e sem o subagente das etapas do plano. O pedido vai pelo stdin, não como argumento.
    #[test]
    fn background_run_keeps_the_workspace_and_skips_the_conversation_binding() {
        let request = background_request(
            "revisar os testes".into(),
            std::path::PathBuf::from("C:\\projeto"),
            Permission::AcceptEdits,
            None,
        );

        assert_eq!(request.cwd, std::path::PathBuf::from("C:\\projeto"));
        assert_eq!(request.permission, Permission::AcceptEdits);
        assert!(request.session_id.is_none());
        assert!(!request.subagents);

        let flags = background_args(&request);
        assert!(!flags.iter().any(|arg| arg == "--resume"), "não retoma a conversa: {flags:?}");
        assert!(!flags.iter().any(|arg| arg == "--agents"), "sem o papel luna-etapa: {flags:?}");
        assert!(flags.windows(2).any(|window| window == ["--permission-mode", "default"]));
        assert!(!flags.iter().any(|arg| arg.contains("revisar os testes")), "o pedido vai pelo stdin: {flags:?}");
        assert_eq!(input(&request).unwrap(), "revisar os testes");
    }
}
