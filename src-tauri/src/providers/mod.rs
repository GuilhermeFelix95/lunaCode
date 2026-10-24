pub mod antigravity;
pub mod claude;
pub mod cline;
pub mod codex;
pub mod copilot;
pub mod hermes;
pub mod opencode;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// CLIs oficiais suportados pelo harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Claude,
    Codex,
    OpenCode,
    Cline,
    Copilot,
    Antigravity,
    Hermes,
}

/// Navegador do agente, escolhido em Configurações. Só um vale por execução.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentBrowser {
    #[default]
    Builtin,
    Playwright,
}

/// Permissão escolhida no composer. Cada CLI tem o próprio nome para ela (ver claude.rs e codex.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    ReadOnly,
    AcceptEdits,
    FullAccess,
}

/// Uma execução do CLI: o que enviar, em qual conversa, em qual pasta e com qual permissão.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    pub provider: ProviderKind,
    pub prompt: String,
    /// Id devolvido pelo CLI na primeira mensagem. Ausente = conversa nova.
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub permission: Permission,
    /// Navegador que o agente usa nesta execução. Ausente = o navegador embutido.
    #[serde(default)]
    pub browser: AgentBrowser,
    /// Modo navegador: a Luna recebe as instruções do navegador (ver BROWSER_IDENTITY). Ausente = modo LunaCode.
    #[serde(default)]
    pub browser_mode: bool,
    /// Etapas delegadas ao subagente do orquestrador (só o Claude usa). Ausente = sem subagente.
    #[serde(default)]
    pub subagents: bool,
    /// Modelo escolhido na lista que a própria CLI expõe. Ausente = padrão da CLI.
    #[serde(default)]
    pub model: Option<String>,
    /// Nível de esforço de raciocínio, com o nome que a CLI usa (ex.: "high", "xhigh").
    #[serde(default)]
    pub effort: Option<String>,
    /// Perfil de API (id em api-profiles.json). Ausente = login da própria CLI.
    /// A chave não vem aqui: o app a lê do cofre de credenciais.
    #[serde(default)]
    pub profile: Option<String>,
    /// Imagens anexadas (PNG, JPEG ou WebP) na pasta de anexos do app. Ausente = sem imagem.
    #[serde(default)]
    pub images: Vec<PathBuf>,
    /// Tamanho da janela de contexto do modelo escolhido, em tokens. Ausente = o app não sabe.
    #[serde(default)]
    pub context_window: Option<u64>,
    /// Cache de prompt de 1 h (só perfis de chave Anthropic). Ausente/false = o TTL padrão de 5 min.
    #[serde(default)]
    pub long_cache: bool,
}

impl RunRequest {
    /// Recusa modelo ou esforço que não sejam um nome simples, antes de virarem argumento do CLI.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(model) = &self.model {
            if !is_model_token(model) {
                return Err(format!("modelo inválido: {model:?}"));
            }
        }
        if let Some(effort) = &self.effort {
            if !is_plain_token(effort) {
                return Err(format!("esforço inválido: {effort:?}"));
            }
        }
        // O Cline não tem modo somente leitura nem pede aprovação no modo JSON: só o acesso total é verdadeiro.
        if self.provider == ProviderKind::Cline && self.permission != Permission::FullAccess {
            return Err("O Cline só roda com acesso total: ele não tem modo somente leitura nem pede aprovação.".into());
        }
        if self.provider == ProviderKind::Hermes {
            hermes::check_permission(self.permission)?;
        }
        if self.images.len() > MAX_IMAGES {
            return Err(format!("no máximo {MAX_IMAGES} imagens por pedido"));
        }
        if let Some(bad) = self.images.iter().find(|p| image_media_type(p).is_none()) {
            return Err(format!("formato de imagem não suportado: {}", bad.display()));
        }
        Ok(())
    }
}

/// Letras, números, `.`, `_` e `-`, sem começar por `-` (não pode parecer flag do CLI).
pub(crate) fn is_plain_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Nome de modelo: como `is_plain_token`, mas aceita `/` e `:` (slugs de agregadores, ex.: `z-ai/glm-5.2:free`).
pub fn is_model_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':'))
}

/// Evento interno único. O frontend nunca vê o JSON bruto de cada CLI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ProviderEvent {
    SessionStarted { session_id: String },
    TextDelta { text: String },
    /// Ferramenta chamada pelo agente. `id` liga o chamado ao resultado (vazio quando o CLI não informa).
    ToolUse { id: String, name: String, detail: String },
    /// Resultado de uma ferramenta. Dentro do `CommandTracker`, a saída vai vazia: o chat só precisa do estado.
    ToolResult { id: String, ok: bool, output: String },
    /// Resultado de um comando do agente (Bash). `output` guarda só o final da saída.
    CommandResult { ok: bool, output: String },
    /// Modelo que respondeu esta mensagem, como o próprio CLI informa. Só o Claude informa hoje.
    ModelUsed { model: String },
    /// Etapas que o agente escreveu com a ferramenta TodoWrite, na ordem dele. A lista inteira vale a cada evento.
    Todos { items: Vec<TodoItem> },
    /// Tarefa criada com TaskCreate. `key` é o id do tool_use, que liga ao `TaskAssigned` seguinte.
    TaskCreated { key: String, subject: String },
    /// Id que o Claude Code deu à tarefa criada (vem no resultado da ferramenta).
    TaskAssigned { key: String, id: String },
    /// Mudança de uma tarefa com TaskUpdate: estado novo (pending, in_progress, completed ou deleted) e/ou novo nome.
    TaskUpdated { id: String, status: Option<String>, subject: Option<String> },
    /// Uso de uma janela de limite da conta (sessão de 5 horas ou semana). `utilization` vem de 0 a 1.
    RateLimit { window: LimitWindow, utilization: f64, resets_at: i64 },
    /// Tokens que ocupam a janela de contexto no último turno, e o tamanho dessa janela.
    Context { tokens: u64, window_size: Option<u64> },
    /// Tokens de entrada do turno, separados em novos, lidos do cache e gravados no cache.
    /// `None` quando o provedor não informa o campo: é diferente de zero.
    CacheUsage { fresh: Option<u64>, read: Option<u64>, write: Option<u64> },
    /// Diff de uma edição de arquivo. `id` liga à ferramenta que fez a edição (o `ToolUse` anterior).
    ToolDiff { id: String, diff: FileDiff },
    /// Tokens de saída de uma chamada ao modelo que terminou. A soma das chamadas do turno é a velocidade medida.
    OutputTokens { tokens: u64 },
    /// Custo do turno em dólares, quando a CLI informa (uso com API). Com assinatura, a CLI não informa.
    Cost { usd: f64 },
    Error { message: String },
    Done { ok: bool },
}

/// Uma etapa do checklist. `status` vem como o agente escreveu: pending, in_progress ou completed.
/// Linhas de uma edição. `added` e `removed` contam todas as linhas; `lines` pode vir cortada.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub added: usize,
    pub removed: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffKind {
    Add,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    pub content: String,
    pub status: String,
}

/// Janelas de limite que o Claude informa. O nome vai para o frontend como está aqui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LimitWindow {
    FiveHour,
    SevenDay,
}

/// Máximo de imagens por pedido.
pub const MAX_IMAGES: usize = 5;

/// Identidade do agente, qualquer que seja o modelo ou o motor. Vai como instrução extra em cada CLI.
/// Sem aspas nem quebras de linha: o Codex recebe o texto dentro de uma string TOML.
pub const LUNA_IDENTITY: &str = "Você é a Luna, o assistente do aplicativo LunaCode. Essa identidade vale em toda tarefa, inclusive ao criar plugins, escrever código ou documentos. Ao criar um plugin, ele é um plugin do LunaHarness, nunca do Claude Code. Você se apresenta como Luna, mesmo que o prompt padrão diga o contrário. Quando perguntarem qual modelo ou janela de contexto você usa, responda com os dados informados no fim desta instrução.Plugins do LunaHarness são uma pasta com manifest.json (id, name, version, entry, permissions) e um index.js que exporta function activate(api). Use só estas chamadas: api.registerCommand(nome, fn) com a permissão commands; api.onBeforeSend(fn) e api.onAfterResponse(fn) com hooks; api.registerMode({ id, label, description, instruction, permission: readOnly, questions }) com modes. Não invente outras APIs. O guia completo está em docs/plugins.md.";

/// Instruções da Luna no modo navegador: só o que o modo faz hoje, sem o guia de plugins nem as regras do LunaCode.
pub const BROWSER_IDENTITY: &str = "Você é a Luna, a assistente do modo navegador do LunaCode. Você ajuda a entender e a usar a página aberta no navegador do app. Você pode resumir a página, explicar o conteúdo dela, preencher formulários e agir na página quando a pessoa pedir: abrir um endereço, listar os elementos da página com referências, clicar, digitar, rolar, apertar teclas como Esc, esperar um texto aparecer e fechar modais. Ações sensíveis (comprar, pagar, enviar, publicar, excluir, confirmar) só acontecem com a permissão da pessoa. Se a página pedir login, CAPTCHA ou confirmação de conta, pare e peça que a pessoa resolva na página. Você também pode pesquisar na web pelo motor escolhido nas configurações do navegador, e ler os links dos resultados. Você não gerencia favoritos nem histórico: se perguntarem, diga que isso não é feito por você e que a pessoa usa a barra de favoritos e o histórico do navegador. Ao falar do que pode fazer, cite só estas capacidades. Não fale de plugins, modos nem do guia do LunaCode.";

/// Instrução de economia de saída, no fim das identidades: menos tokens gastos e resposta mais útil.
/// Sem aspas nem quebra de linha, como o resto da identidade (o Codex recebe o texto em string TOML).
const CONCISE: &str = " Responda com concisão: vá direto ao ponto em poucas linhas, sem repetir o que já foi dito, sem resumir o próprio trabalho e sem listar o óbvio; só detalhe quando pedirem.";

/// Identidade mais os dados da conversa: modelo, esforço e janela de contexto. Sem aspas nem quebras de linha.
pub fn identity_for(request: &RunRequest) -> String {
    let model = match &request.model {
        Some(model) => format!("Modelo desta conversa: {model}."),
        None => "Modelo desta conversa: o padrão da CLI.".to_string(),
    };
    let effort = match &request.effort {
        Some(effort) => format!(" Esforço: {effort}."),
        None => String::new(),
    };
    let window = match request.context_window {
        Some(tokens) => format!(" Janela de contexto: {tokens} tokens."),
        None => String::new(),
    };
    // No modo navegador a identidade é a do navegador: não leva o guia nem a pasta de rascunhos de plugins.
    if request.browser_mode {
        return format!("{BROWSER_IDENTITY} {model}{effort}{window}{CONCISE}");
    }
    // Caminho do guia, para o agente ler antes de criar um plugin (ver plugins/guide.rs).
    let guide = match crate::plugins::guide::dir() {
        Some(dir) => format!(" Guia de plugins: {}.", dir.join("plugins.md").display()),
        None => String::new(),
    };
    // Pasta onde criar o plugin: a Luna escreve ali e o app instala pelo cartão no chat (ver plugins/guide.rs).
    let drafts = match crate::plugins::guide::drafts_dir() {
        Some(dir) => format!(" Pasta de rascunhos de plugin: {}.", dir.display()),
        None => String::new(),
    };
    format!("{LUNA_IDENTITY} {model}{effort}{window}{guide}{drafts}{CONCISE}")
}

/// O navegador do agente só entra no turno quando está em uso: modo navegador ligado ou o painel do
/// navegador já aberto nesta execução do app (ver `browser::is_used`). Fora disso, as 9 ferramentas
/// do MCP ficavam no prefixo de todo turno sem ninguém pedir — tokens gastos à toa.
pub fn browser_in_use(request: &RunRequest) -> bool {
    request.browser_mode || crate::browser::is_used()
}

/// Tipo MIME de uma imagem aceita, pela extensão. \`None\` = formato não suportado.
pub fn image_media_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// O que o CLI lê pelo stdin: o prompt puro, ou (Claude com imagens) uma linha stream-json.
pub fn stdin_input(request: &RunRequest) -> Result<String, String> {
    match request.provider {
        ProviderKind::Claude => claude::input(request),
        ProviderKind::Codex => Ok(request.prompt.clone()),
        ProviderKind::OpenCode => Ok(opencode::prompt(request)),
        // O Cline recebe o prompt por arquivo (ver `Session::start_with_env`); o stdin fica vazio.
        ProviderKind::Cline => Ok(String::new()),
        ProviderKind::Copilot => Ok(request.prompt.clone()),
        // O Antigravity recebe o prompt colado ao `-p` (ver providers/antigravity.rs); o stdin fica vazio.
        ProviderKind::Antigravity => Ok(String::new()),
        ProviderKind::Hermes => Ok(request.prompt.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A identidade não depende de alguém perguntar quem é: vale em toda tarefa, inclusive ao criar plugins.
    #[test]
    fn identity_holds_in_every_task_including_creating_plugins() {
        assert!(LUNA_IDENTITY.contains("em toda tarefa"));
        assert!(LUNA_IDENTITY.contains("criar plugins"));
        // O guia de plugins vai junto: uma linha, sem aspas nem % (ver no_argument_has_line_breaks...).
        assert!(LUNA_IDENTITY.contains("api.registerCommand"));
        assert!(!LUNA_IDENTITY.contains('%'));
        // Sem proibição de dizer o modelo: a Luna responde com os dados da conversa.
        assert!(!LUNA_IDENTITY.contains("Nunca diga"));
        assert!(LUNA_IDENTITY.contains("Quando perguntarem qual modelo"));
    }

    #[test]
    fn identity_carries_the_model_effort_and_context_window() {
        let mut req = request(ProviderKind::Claude, Permission::ReadOnly, None);
        req = with_model(req, "claude-opus-5-5", "xhigh");
        req.context_window = Some(200_000);

        let identity = identity_for(&req);
        assert!(identity.starts_with(LUNA_IDENTITY));
        assert!(identity.contains("Modelo desta conversa: claude-opus-5-5."));
        assert!(identity.contains("Esforço: xhigh."));
        assert!(identity.contains("Janela de contexto: 200000 tokens."));
        assert!(!identity.contains('"') && !identity.contains('%') && !identity.contains('\n'));
    }

    #[test]
    fn identity_without_model_says_the_cli_chooses() {
        let req = request(ProviderKind::Codex, Permission::ReadOnly, None);
        let identity = identity_for(&req);
        assert!(identity.contains("o padrão da CLI"));
        assert!(!identity.contains("Janela de contexto"));
    }

    /// No modo navegador a Luna recebe as próprias instruções: sem o guia de plugins nem as regras do LunaCode.
    #[test]
    fn browser_mode_gets_its_own_identity_without_the_luna_code_rules() {
        let mut req = request(ProviderKind::Codex, Permission::ReadOnly, None);
        req.browser_mode = true;

        let identity = identity_for(&req);

        assert!(identity.starts_with(BROWSER_IDENTITY));
        assert!(BROWSER_IDENTITY.contains("resumir"));
        assert!(BROWSER_IDENTITY.contains("preencher formulários"));
        assert!(BROWSER_IDENTITY.contains("não gerencia favoritos nem histórico"));
        assert!(!identity.contains("api.registerCommand"));
        assert!(!identity.contains("LunaHarness"));
    }

    #[test]
    fn luna_code_mode_keeps_the_luna_code_identity() {
        let req = request(ProviderKind::Codex, Permission::ReadOnly, None);

        assert!(identity_for(&req).starts_with(LUNA_IDENTITY));
    }

    /// Economia de saída: as duas identidades pedem resposta curta, sem aspas nem quebra de linha.
    #[test]
    fn identity_asks_for_concise_answers_in_both_modes() {
        let mut req = request(ProviderKind::Claude, Permission::ReadOnly, None);
        for browser_mode in [false, true] {
            req.browser_mode = browser_mode;
            let identity = identity_for(&req);
            assert!(identity.contains("concisão"), "modo navegador: {browser_mode}");
            assert!(!identity.contains('\n') && !identity.contains('"'), "modo navegador: {browser_mode}");
        }
    }

    /// O navegador do agente entra no turno só quando em uso: modo navegador ligado ou painel já aberto.
    #[test]
    fn browser_is_in_use_only_in_browser_mode_or_after_the_panel_opened() {
        let mut req = request(ProviderKind::Claude, Permission::ReadOnly, None);
        assert_eq!(browser_in_use(&req), crate::browser::is_used());

        req.browser_mode = true;
        assert!(browser_in_use(&req));
    }

    fn request(provider: ProviderKind, permission: Permission, session_id: Option<&str>) -> RunRequest {
        RunRequest {
            provider,
            prompt: "oi".into(),
            session_id: session_id.map(String::from),
            cwd: PathBuf::from("C:\\projeto"),
            permission,
            model: None,
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

    fn with_model(mut request: RunRequest, model: &str, effort: &str) -> RunRequest {
        request.model = Some(model.into());
        request.effort = Some(effort.into());
        request
    }

    #[test]
    fn model_and_effort_must_be_plain_tokens() {
        let mut req = request(ProviderKind::Claude, Permission::ReadOnly, None);
        assert!(req.validate().is_ok());

        req = with_model(req, "claude-opus-5-5", "xhigh");
        assert!(req.validate().is_ok());

        // Nada que pareça flag ou traga espaço/aspas chega aos argumentos do CLI.
        assert!(with_model(request(ProviderKind::Codex, Permission::ReadOnly, None), "--help", "high")
            .validate()
            .is_err());
        assert!(with_model(request(ProviderKind::Codex, Permission::ReadOnly, None), "gpt 5", "high")
            .validate()
            .is_err());
        assert!(with_model(request(ProviderKind::Codex, Permission::ReadOnly, None), "gpt-5", "high\"x")
            .validate()
            .is_err());
    }

    #[test]
    fn model_names_accept_slashes_and_colons_from_aggregators() {
        let req = with_model(request(ProviderKind::Claude, Permission::ReadOnly, None), "z-ai/glm-5.2:free", "high");
        assert!(req.validate().is_ok());

        // Caracteres de shell continuam de fora, mesmo com as barras liberadas.
        assert!(with_model(request(ProviderKind::Claude, Permission::ReadOnly, None), "a&b", "high").validate().is_err());
    }

    /// Tira a instrução da identidade, para estes testes cobrirem só modo, sandbox e modelo.
    fn without_identity(mut args: Vec<String>) -> Vec<String> {
        if let Some(at) = args.iter().position(|a| a == "--append-system-prompt") {
            args.drain(at..at + 2);
        }
        if let Some(at) = args.iter().position(|a| a.starts_with("developer_instructions=")) {
            args.drain(at - 1..at + 1);
        }
        // O servidor de etapas tem o caminho do executável, que muda a cada build: os testes comparam sem ele.
        while let Some(at) = args.iter().position(|a| a.starts_with(&format!("mcp_servers.{}.", crate::tasks_mcp::SERVER))) {
            args.drain(at - 1..at + 1);
        }
        args
    }

    #[test]
    fn claude_new_conversation_args_use_stream_and_permission_mode() {
        let args = without_identity(cli_args(&request(ProviderKind::Claude, Permission::ReadOnly, None)));

        assert_eq!(
            args,
            [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--permission-mode",
                "plan",
            ]
        );
    }

    #[test]
    fn claude_permission_modes_map_to_official_flags() {
        let mode = |p| {
            let args = cli_args(&request(ProviderKind::Claude, p, None));
            args[args.iter().position(|a| a == "--permission-mode").unwrap() + 1].clone()
        };

        assert_eq!(mode(Permission::ReadOnly), "plan");
        assert_eq!(mode(Permission::AcceptEdits), "default");
        assert_eq!(mode(Permission::FullAccess), "bypassPermissions");
    }

    #[test]
    fn claude_resume_adds_session_flag() {
        let args = cli_args(&request(ProviderKind::Claude, Permission::AcceptEdits, Some("abc-123")));

        let pos = args.iter().position(|a| a == "--resume").expect("--resume presente");
        assert_eq!(args[pos + 1], "abc-123");
    }

    #[test]
    fn codex_new_conversation_uses_sandbox_flag_and_stdin_prompt() {
        let args = without_identity(cli_args(&request(ProviderKind::Codex, Permission::AcceptEdits, None)));

        assert_eq!(
            args,
            ["exec", "--json", "--skip-git-repo-check", "-s", "read-only", "-"]
        );
    }

    #[test]
    fn codex_resume_passes_sandbox_as_config_override() {
        let args = without_identity(cli_args(&request(ProviderKind::Codex, Permission::ReadOnly, Some("t-1"))));

        assert_eq!(
            args,
            [
                "exec",
                "-c",
                "sandbox_mode=\"read-only\"",
                "resume",
                "t-1",
                "--json",
                "--skip-git-repo-check",
                "-",
            ]
        );
    }

    #[test]
    fn codex_full_access_maps_to_danger_sandbox() {
        let args = cli_args(&request(ProviderKind::Codex, Permission::FullAccess, None));

        let pos = args.iter().position(|a| a == "-s").unwrap();
        assert_eq!(args[pos + 1], "danger-full-access");
    }

    #[test]
    fn claude_passes_model_and_effort_as_official_flags() {
        let req = with_model(request(ProviderKind::Claude, Permission::AcceptEdits, None), "opus", "max");
        let args = cli_args(&req);

        let model = args.iter().position(|a| a == "--model").expect("--model presente");
        assert_eq!(args[model + 1], "opus");
        let effort = args.iter().position(|a| a == "--effort").expect("--effort presente");
        assert_eq!(args[effort + 1], "max");
    }

    #[test]
    fn codex_passes_model_flag_and_reasoning_effort_override() {
        let req = with_model(request(ProviderKind::Codex, Permission::AcceptEdits, None), "gpt-5.5", "xhigh");

        assert_eq!(
            without_identity(cli_args(&req)),
            [
                "exec",
                "--json",
                "--skip-git-repo-check",
                "-s",
                "read-only",
                "-m",
                "gpt-5.5",
                "-c",
                "model_reasoning_effort=\"xhigh\"",
                "-",
            ]
        );
    }

    #[test]
    fn codex_resume_keeps_model_and_effort_before_the_subcommand() {
        let req = with_model(request(ProviderKind::Codex, Permission::ReadOnly, Some("t-1")), "gpt-5.5", "low");

        assert_eq!(
            without_identity(cli_args(&req)),
            [
                "exec",
                "-c",
                "sandbox_mode=\"read-only\"",
                "-m",
                "gpt-5.5",
                "-c",
                "model_reasoning_effort=\"low\"",
                "resume",
                "t-1",
                "--json",
                "--skip-git-repo-check",
                "-",
            ]
        );
    }

    #[test]
    fn fallback_after_silent_failure_reports_reason_and_closes_turn() {
        let events = fallback_events(ProviderKind::Codex, Some(3), "token expirado");

        assert_eq!(
            events,
            vec![
                ProviderEvent::Error {
                    message: "Codex terminou sem concluir a resposta (código 3): token expirado"
                        .into()
                },
                ProviderEvent::Done { ok: false },
            ]
        );
    }

    #[test]
    fn result_with_api_cost_reports_it_before_done() {
        let line = r#"{"type":"result","is_error":false,"total_cost_usd":0.0421,"result":"ok"}"#;
        let events = parse_line(ProviderKind::Claude, line);

        assert_eq!(
            events,
            vec![ProviderEvent::Cost { usd: 0.0421 }, ProviderEvent::Done { ok: true }]
        );
    }

    #[test]
    fn result_without_cost_keeps_the_old_events() {
        let line = r#"{"type":"result","is_error":false,"result":"ok"}"#;
        assert_eq!(parse_line(ProviderKind::Claude, line), vec![ProviderEvent::Done { ok: true }]);
    }

    #[test]
    fn serialized_event_matches_frontend_contract() {
        let json = serde_json::to_value(ProviderEvent::SessionStarted {
            session_id: "abc".into(),
        })
        .unwrap();

        // src/types.ts espera { kind: "sessionStarted", sessionId }.
        assert_eq!(json["kind"], "sessionStarted");
        assert_eq!(json["sessionId"], "abc");
    }
    #[test]
    fn validate_rejects_unknown_image_types_and_too_many_images() {
        let mut r = request(ProviderKind::Claude, Permission::ReadOnly, None);
        r.images = vec![PathBuf::from("C:\\x\\foto.gif")];
        assert!(r.validate().is_err());
        r.images = vec![PathBuf::from("C:\\x\\foto.png"); MAX_IMAGES + 1];
        assert!(r.validate().is_err());
        r.images = vec![PathBuf::from("C:\\x\\foto.webp")];
        assert!(r.validate().is_ok());
    }
}

/// Quantos caracteres do fim da saída de um comando vão para a verificação (o erro costuma estar no fim).
pub const COMMAND_OUTPUT_MAX_CHARS: usize = 4000;

/// Os últimos `max` caracteres do texto (não bytes, para não cortar um acento no meio).
pub fn last_chars(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max { text.to_string() } else { text.chars().skip(count - max).collect() }
}

/// Liga os chamados de Bash aos resultados. O Claude manda o resultado numa linha separada do chamado,
/// então é preciso lembrar quais ids eram comandos.
#[derive(Default)]
pub struct CommandTracker {
    commands: std::collections::HashSet<String>,
}

impl CommandTracker {
    /// Eventos para o chat. Todo resultado de ferramenta vira `ToolResult` sem a saída (o chat mostra o estado);
    /// resultado de Bash também vira `CommandResult`, com o fim da saída, para o LunaFix.
    pub fn observe(&mut self, event: ProviderEvent) -> Vec<ProviderEvent> {
        match event {
            ProviderEvent::ToolUse { ref id, ref name, .. } => {
                if name == "Bash" && !id.is_empty() {
                    self.commands.insert(id.clone());
                }
                vec![event]
            }
            ProviderEvent::ToolResult { id, ok, output } => {
                let mut events = Vec::new();
                if self.commands.remove(&id) {
                    events.push(ProviderEvent::CommandResult {
                        ok,
                        output: last_chars(&output, COMMAND_OUTPUT_MAX_CHARS),
                    });
                }
                events.push(ProviderEvent::ToolResult { id, ok, output: String::new() });
                events
            }
            other => vec![other],
        }
    }
}

/// Traduz uma linha de saída do CLI para eventos internos.
/// Linhas que não carregam informação útil para o chat são ignoradas.
pub fn parse_line(kind: ProviderKind, line: &str) -> Vec<ProviderEvent> {
    match kind {
        ProviderKind::Claude => claude::parse_line(line),
        ProviderKind::Codex => codex::parse_line(line),
        ProviderKind::OpenCode => opencode::parse_line(line),
        ProviderKind::Cline => cline::parse_line(line),
        ProviderKind::Copilot => copilot::parse_line(line),
        ProviderKind::Antigravity => antigravity::parse_line(line),
        ProviderKind::Hermes => hermes::parse_line(line),
    }
}

/// Argumentos do CLI para uma execução. O prompt vai sempre pelo stdin, nunca como argumento,
/// para evitar problemas de escape no Windows (os CLIs são shims `.cmd`).
pub fn cli_args(request: &RunRequest) -> Vec<String> {
    match request.provider {
        ProviderKind::Claude => claude::args(request),
        ProviderKind::Codex => codex::args(request),
        ProviderKind::OpenCode => opencode::args(request),
        // Os argumentos do Cline levam o caminho do arquivo do prompt, que só existe na hora de gravar (ver session.rs).
        ProviderKind::Cline => Vec::new(),
        ProviderKind::Copilot => copilot::args(request),
        ProviderKind::Antigravity => antigravity::args(request),
        ProviderKind::Hermes => hermes::args(request),
    }
}

/// Nome legível do provedor, usado em mensagens para o usuário.
pub fn label(kind: ProviderKind) -> &'static str {
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

/// Eventos finais quando o CLI termina sem `Done` (crash, falta de login, rede caiu).
/// Garante que o botão "Parar" destrava e que o usuário vê o motivo.
pub fn fallback_events(kind: ProviderKind, exit_code: Option<i32>, stderr: &str) -> Vec<ProviderEvent> {
    let code = exit_code.map_or_else(|| "sem código de saída".to_string(), |c| format!("código {c}"));
    let detail = if stderr.is_empty() {
        String::new()
    } else {
        format!(": {stderr}")
    };
    vec![
        ProviderEvent::Error {
            message: format!("{} terminou sem concluir a resposta ({code}){detail}", label(kind)),
        },
        ProviderEvent::Done { ok: false },
    ]
}

/// Nome do executável como aparece no PATH.
pub fn cli_name(kind: ProviderKind) -> &'static str {
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


#[cfg(test)]
mod command_result_tests {
    use super::*;

    fn tool_use(id: &str, name: &str) -> ProviderEvent {
        ProviderEvent::ToolUse { id: id.into(), name: name.into(), detail: "exit 3".into() }
    }

    /// Só o resultado de Bash vira CommandResult; todo resultado segue para o chat como ToolResult, sem a saída.
    #[test]
    fn only_bash_results_become_command_results() {
        let mut tracker = CommandTracker::default();

        assert_eq!(tracker.observe(tool_use("t1", "Bash")), vec![tool_use("t1", "Bash")]);
        assert_eq!(tracker.observe(tool_use("t2", "Read")), vec![tool_use("t2", "Read")]);
        assert_eq!(
            tracker.observe(ProviderEvent::ToolResult { id: "t2".into(), ok: false, output: "arquivo não existe".into() }),
            vec![ProviderEvent::ToolResult { id: "t2".into(), ok: false, output: String::new() }]
        );
        assert_eq!(
            tracker.observe(ProviderEvent::ToolResult { id: "t1".into(), ok: false, output: "Exit code 3".into() }),
            vec![
                ProviderEvent::CommandResult { ok: false, output: "Exit code 3".into() },
                ProviderEvent::ToolResult { id: "t1".into(), ok: false, output: String::new() },
            ]
        );
    }

    /// Saída longa: vale o final, onde costuma estar o erro, e o corte respeita caracteres (não bytes).
    #[test]
    fn command_output_keeps_only_the_last_4000_characters() {
        let long = format!("{}erro final", "á".repeat(5000));
        let mut tracker = CommandTracker::default();
        tracker.observe(tool_use("t1", "Bash"));

        let events = tracker.observe(ProviderEvent::ToolResult { id: "t1".into(), ok: false, output: long });
        let Some(ProviderEvent::CommandResult { output, .. }) = events.first() else {
            panic!("esperava CommandResult");
        };

        assert_eq!(output.chars().count(), 4000);
        assert!(output.ends_with("erro final"));
    }
}
