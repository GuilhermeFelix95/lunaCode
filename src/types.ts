// Espelha src-tauri/src/providers/mod.rs. Mantenha os dois lados em sincronia.

export type ProviderKind = "claude" | "codex" | "opencode" | "cline" | "copilot" | "antigravity" | "hermes";

/// Permissão escolhida no composer. Cada CLI recebe o modo equivalente (ver providers/claude.rs e codex.rs).
export type Permission = "readOnly" | "acceptEdits" | "fullAccess";

/// Navegador do agente, escolhido em Configurações > Navegador. Só um vale por vez.
export type AgentBrowser = "builtin" | "playwright";

/// Motor de busca da barra de endereço do navegador embutido. Padrão: o Google.
export type SearchEngine = "google" | "duckduckgo" | "bing" | "brave";

export interface RunRequest {
  provider: ProviderKind;
  prompt: string;
  /** Id da conversa no CLI. Ausente = conversa nova. */
  sessionId?: string;
  cwd: string;
  permission: Permission;
  /** Navegador do agente nesta execução. Ausente = o embutido. */
  browser?: AgentBrowser;
  /** Modo navegador: a Luna recebe as instruções do navegador. Ausente = modo LunaCode. */
  browserMode?: boolean;
  /** Etapas delegadas ao subagente do orquestrador (só o Claude). Ausente = sem subagente. */
  subagents?: boolean;
  /** Modo agente: pesquisa na web (só WebSearch) sem pedido a cada uso. Ausente = não. */
  webSearch?: boolean;
  /** Modelo da lista da CLI. Ausente = padrão da CLI. */
  model?: string;
  /** Nível de esforço com o nome da CLI. Ausente = padrão. */
  effort?: string;
  /** Janela de contexto do modelo, em tokens. Entra na identidade para a Luna saber responder. */
  contextWindow?: number;
  /** Cache de prompt de 1 h (só chave da Anthropic). Ausente = o padrão de 5 min. */
  longCache?: boolean;
  /** Perfil de API (id em api-profiles.json). Ausente = login da própria CLI. A chave fica no cofre. */
  profile?: string;
  /** Caminhos das imagens anexadas, salvas pelo app (save_attachment). */
  images?: string[];
}

/// Perfil de API salvo, sem a chave (ver src-tauri/src/api_profiles.rs).
export interface ApiProfile {
  id: string;
  name: string;
  preset: string;
  /** Vazio = endereço padrão do preset. */
  baseUrl: string;
  models: string[];
  defaultModel: string | null;
  /** Só OpenRouter: "free" mostra no chat apenas os modelos com :free. */
  modelScope?: "all" | "free";
  /** Janela de contexto em tokens, por id de modelo, quando a API informou. */
  contextWindows?: Record<string, number>;
}

/// Provedor pronto para criar um perfil (ver `api_profiles::presets`).
/// Modelos que a API do provedor devolve. `cheapest` é null quando a API não informa preço.
export interface RemoteModels {
  ids: string[];
  cheapest: string | null;
  /** Janela de contexto em tokens, por id, quando a API informa. */
  contexts: Record<string, number>;
}

export interface ApiPreset {
  preset: string;
  label: string;
  engine: ProviderKind;
  defaultBaseUrl: string;
  /** Modelos que já vêm prontos no preset: o usuário só cola a chave. */
  suggestedModels: string[];
  /** Modelo padrão entre os sugeridos. Vazio = nenhum. */
  defaultModel: string;
  /** Modelos do provedor que o Claude Code não usa: aparecem marcados, não entram na lista. */
  unsupportedModels?: string[];
}

/// Nível de esforço de um modelo (ver src-tauri/src/models.rs). `id` é o valor enviado à CLI.
export interface EffortOption {
  id: string;
  label: string;
  recommended: boolean;
}

/// Modelo listado para o seletor (ver src-tauri/src/models.rs).
export interface ModelOption {
  id: string;
  label: string;
  description: string | null;
  efforts: EffortOption[];
  /** Modelo escolhido quando a conversa não tem escolha: o mais barato do provedor. */
  default: boolean;
  /** Janela de contexto em tokens, quando a API informa. */
  contextWindow?: number;
}

/// Conta extra de uma CLI (espelha src-tauri/src/accounts.rs). Sem segredo: o login fica na pasta da conta.
export interface Account {
  id: string;
  provider: ProviderKind;
  name: string;
}

/// Situação de uma CLI no computador (espelha setup::CliStatus em src-tauri/src/setup.rs).
/// Item da árvore de arquivos do projeto. `path` é relativo à pasta do projeto, com "/".
export interface ProjectEntry {
  name: string;
  path: string;
  isDir: boolean;
}

/// Um arquivo ou pasta do projeto que as CLIs leem, e se ele existe na pasta.
export interface ProjectFile {
  name: string;
  found: boolean;
  purpose: string;
}

export interface CliStatus {
  provider: ProviderKind;
  installed: boolean;
  /** Onde a CLI foi achada. Null = não instalada. */
  path: string | null;
}

/** Versão instalada e a mais nova de uma CLI. `updateAvailable` só sai com fonte de versão confirmada. */
export interface CliUpdateInfo {
  provider: ProviderKind;
  installed: string | null;
  latest: string | null;
  updateAvailable: boolean;
  /** Tem comando de atualização confirmado: o app oferece o botão. */
  canUpdate: boolean;
  error: string | null;
}

/// Tarefa salva na conversa: `key` liga o TaskCreate ao resultado, `id` é o id que o TaskUpdate usa.
export interface SavedTask {
  key: string;
  id?: string;
  content: string;
  status: TodoItem["status"];
}

/// Uma etapa do checklist do agente (espelha TodoItem em providers/mod.rs).
export interface TodoItem {
  content: string;
  status: "pending" | "in_progress" | "completed";
}

/// Situação de uma tarefa em segundo plano (espelha background_tasks::TaskStatus).
export type BackgroundTaskStatus = "running" | "done" | "failed" | "stopped";

/// Tarefa em segundo plano: execução própria do Claude CLI, com status e saída (ver background_tasks.rs).
export interface BackgroundTask {
  id: string;
  prompt: string;
  status: BackgroundTaskStatus;
  output: string;
  /** Conversa que pediu a tarefa: a lista e o resultado ficam só nela. */
  conversationId: string;
}

/** Edição de arquivo: contagens exatas de linhas, e só as primeiras linhas vêm para o chat. */
export interface FileDiff {
  added: number;
  removed: number;
  lines: { kind: "add" | "remove"; text: string }[];
}

export type ProviderEvent =
  | { kind: "sessionStarted"; sessionId: string }
  | { kind: "textDelta"; text: string }
  | { kind: "toolUse"; id: string; name: string; detail: string }
  /** Fim de uma ferramenta: `ok` diz se deu certo. A saída vem vazia (só comandos guardam o fim dela em `commandResult`). */
  | { kind: "toolResult"; id: string; ok: boolean; output: string }
  /** Modelo que respondeu a mensagem (o Claude informa; os outros motores ainda não). */
  | { kind: "modelUsed"; model: string }
  | { kind: "todos"; items: TodoItem[] }
  | { kind: "taskCreated"; key: string; subject: string }
  | { kind: "taskAssigned"; key: string; id: string }
  | { kind: "taskUpdated"; id: string; status: string | null; subject: string | null }
  | { kind: "rateLimit"; window: "fiveHour" | "sevenDay"; utilization: number; resetsAt: number }
  | { kind: "context"; tokens: number; windowSize: number | null }
  /** Tokens de entrada do turno: novos, lidos do cache e gravados. null = o provedor não informou. */
  | { kind: "cacheUsage"; fresh: number | null; read: number | null; write: number | null }
  /** Tokens de saída de uma chamada ao modelo que terminou. Somados no turno, dão a velocidade medida. */
  | { kind: "outputTokens"; tokens: number }
  /** Diff de uma edição de arquivo; `id` liga à ferramenta que a fez. */
  | { kind: "toolDiff"; id: string; diff: FileDiff }
  | { kind: "cost"; usd: number }
  /** Resultado de um comando do agente (Bash): ok e a saída do fim. */
  | { kind: "commandResult"; ok: boolean; output: string }
  | { kind: "error"; message: string }
  | { kind: "done"; ok: boolean };

export interface ChatMessage {
  role: "user" | "assistant";
  text: string;
  /** Linha de ferramenta (ex.: Bash com o comando executado). */
  /** `id` liga à ferramenta que terminou; `ok` fica vazio enquanto ela roda. */
  tool?: { id?: string; name: string; detail: string; ok?: boolean; diff?: FileDiff };
  error?: string;
  /** Agente que respondeu. Ausente em conversas antigas = provedor da conversa. */
  provider?: ProviderKind;
  /** Modelo que respondeu, quando o motor informa. Mostra o papel usado no orquestrador. */
  model?: string;
  /** Nome do agente que respondeu, quando é um perfil de API (ex.: "DeepSeek"). */
  agent?: string;
  /** Estado dos arquivos antes deste turno (só em mensagens do usuário, em repositório git). */
  checkpoint?: string;
  /** Caminhos das imagens enviadas com esta mensagem, na pasta de anexos do app (miniatura no balão). */
  images?: string[];
  /** Hora do envio (usuário) ou da primeira parte da resposta, em ms. Ausente em mensagens antigas. */
  at?: number;
  /** Resposta vinda do cache de respostas: o provedor não foi chamado para ela. */
  cached?: true;
  /** Resultado de uma tarefa em segundo plano: não recebe os pedaços do turno em andamento. */
  background?: true;
}

/// Sessão nativa de um agente e quantas mensagens do histórico ele já viu (ver src/chat/handoff.ts).
export interface AgentSession {
  id: string;
  seen: number;
}

/// Chave: "claude", "codex" ou "api:<id do perfil>" (ver src/chat/agents.ts).
export type AgentSessions = Partial<Record<string, AgentSession>>;

/// Espelha src-tauri/src/conversations.rs (Conversation). Mensagens seguem o formato de ChatMessage.
/** Resumo da conversa compactada. */
export interface Compaction {
  at: number;
  summary: string;
}

export interface Conversation {
  id: string;
  title: string;
  /** Nome escolhido na barra lateral: salvar de novo não troca o título (ver conversations.rs). */
  titleLocked?: boolean;
  provider: ProviderKind;
  sessionId?: string;
  cwd: string;
  permission: Permission;
  model?: string;
  effort?: string;
  planning: boolean;
  /** Modo agente (padrão). Ausente em conversas antigas = ligado. */
  agent?: boolean;
  /** Perfil de API da conversa. Ausente = login da CLI. */
  profile?: string;
  /** Custo acumulado em dólares, quando o uso é por API. */
  costUsd?: number;
  /** Último plano gerado nesta conversa (aba Plano do painel). */
  plan?: string;
  /** Pasta da spec da conversa (specs/NNN-slug). Ausente = ainda não aprovou um plano. */
  specDir?: string;
  /** Conta extra escolhida no composer. Ausente = conta padrão da CLI. */
  account?: string;
  /** Sessões por agente. Ausente em arquivos antigos: vem de sessionId. */
  sessions?: AgentSessions;
  /** Tarefas do agente, para a aba Progresso voltar ao reabrir a conversa. */
  tasks?: SavedTask[];
  /** Compactações: `at` é o tamanho do histórico quando o resumo foi feito (ver chat/compact.ts). */
  compactions?: Compaction[];
  /** Iniciada no modo navegador: some da barra lateral do LunaCode. Ausente = LunaCode. */
  browser?: boolean;
  createdAt: number;
  updatedAt: number;
  messages: ChatMessage[];
}

/// Linha da barra lateral (sem as mensagens).
export interface ConversationSummary {
  id: string;
  title: string;
  provider: ProviderKind;
  cwd: string;
  updatedAt: number;
  /** Iniciada no modo navegador (ver Conversation.browser). */
  browser?: boolean;
}

/// Arquivo alterado na pasta de trabalho (ver src-tauri/src/changes.rs).
export interface ChangedFile {
  path: string;
  status: "added" | "modified" | "deleted" | "renamed" | "untracked";
  additions: number | null;
  deletions: number | null;
}

export interface Changes {
  isRepo: boolean;
  files: ChangedFile[];
}
