import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { PluginManifest } from "./plugins/host";
import type { SlashItem } from "./chat/slash";
import type { Orchestrator } from "./chat/orchestrator";
import type {
  Account,
  ApiPreset,
  ApiProfile,
  BackgroundTask,
  ChangedFile,
  Changes,
  CliStatus,
  CliUpdateInfo,
  ProjectEntry,
  ProjectFile,
  Conversation,
  ConversationSummary,
  ModelOption,
  Permission,
  ProviderEvent,
  ProviderKind,
  RemoteModels,
  RunRequest,
  SearchEngine,
} from "./types";

export const PROVIDER_EVENT = "provider-event";

/// Envia o prompt ao CLI escolhido. A saída chega pelos eventos `provider-event`.
/// `runId` é o id da conversa: cada conversa tem a sua execução, e várias rodam ao mesmo tempo.
/// Escolha de modelo por papel do orquestrador. Sem escolha salva, vem vazio.
export function getOrchestrator(): Promise<Orchestrator> {
  return invoke("get_orchestrator");
}

export function saveOrchestrator(orchestrator: Orchestrator): Promise<void> {
  return invoke("save_orchestrator", { orchestrator });
}

/// `specDir` é a pasta da spec da conversa (specs/NNN-slug). Com ela, o Rust injeta a spec e as decisões no prompt.
/// `account` é a conta extra da execução (id em accounts.json). Ausente = login padrão da CLI.
export function sendPrompt(request: RunRequest, runId: string, specDir?: string, account?: string): Promise<void> {
  return invoke("send_prompt", { request, runId, specDir: specDir ?? null, account: account ?? null });
}

/// Contas extras cadastradas, de todas as CLIs.
export function listAccounts(): Promise<Account[]> {
  return invoke("list_accounts");
}

/// Cria uma conta extra. Ela começa sem login: o usuário loga dentro dela.
export function createAccount(provider: ProviderKind, name: string): Promise<Account> {
  return invoke("create_account", { provider, name });
}

/// Se a conta extra está logada no próprio CLI (usa a pasta dela).
export function accountStatus(account: string): Promise<boolean> {
  return invoke("account_status", { account });
}

/// Abre a janela de login do CLI dentro da pasta da conta extra.
export function accountLogin(account: string): Promise<void> {
  return invoke("account_login", { account });
}

/// Avisa o chat que a lista de contas mudou (a página de Contas cria uma conta).
export const ACCOUNTS_CHANGED = "luna:accounts-changed";

/// Cria a pasta da spec da conversa e devolve o caminho relativo ao projeto (specs/NNN-slug).
export function createSpec(cwd: string, title: string): Promise<string> {
  return invoke("create_spec", { cwd, title });
}

/// Registra uma rodada de perguntas e respostas no decisions.md da spec.
export function recordSpecDecisions(cwd: string, specDir: string, lines: string[]): Promise<void> {
  return invoke("record_spec_decisions", { cwd, specDir, lines });
}

export function cancelPrompt(runId: string): Promise<void> {
  return invoke("cancel_prompt", { runId });
}

/// Entrega só os eventos da execução `runId`.
export function onProviderEvent(
  runId: string,
  handler: (event: ProviderEvent) => void,
): Promise<UnlistenFn> {
  return listen<{ runId: string; event: ProviderEvent }>(PROVIDER_EVENT, (e) => {
    if (e.payload.runId === runId) handler(e.payload.event);
  });
}

/// Pedido de permissão do Claude (espelha permission_prompt::PermissionRequest no Rust).
/// `runId` é o id da conversa que roda o turno.
export interface PermissionRequest {
  id: string;
  runId: string;
  toolName: string;
  input: Record<string, unknown>;
  toolUseId: string;
}

/// Nome do evento de pedido de permissão (espelha permission_prompt::REQUEST_EVENT no Rust).
export const PERMISSION_REQUEST_EVENT = "permission-request";

/// Entrega só os pedidos de permissão da execução `runId`.
export function onPermissionRequest(
  runId: string,
  handler: (request: PermissionRequest) => void,
): Promise<UnlistenFn> {
  return listen<PermissionRequest>(PERMISSION_REQUEST_EVENT, (e) => {
    if (e.payload.runId === runId) handler(e.payload);
  });
}

/// Permite ou nega um pedido de permissão. `message` volta ao agente quando nega.
export function answerPermission(id: string, allow: boolean, message?: string): Promise<void> {
  return invoke("answer_permission", { id, allow, message: message ?? null });
}

/// Nome do evento de tarefa em segundo plano (espelha background_tasks::TASK_EVENT no Rust).
export const BACKGROUND_TASK_EVENT = "background-task";

/// Inicia uma tarefa em segundo plano no Claude. A saída chega pelo evento `background-task`.
/// `conversationId` prende a tarefa à conversa que a pediu: a lista e o resultado ficam só nela.
export function startBackgroundTask(
  prompt: string,
  cwd: string,
  permission: Permission,
  conversationId: string,
): Promise<BackgroundTask> {
  return invoke("start_background_task", { prompt, cwd, permission, conversationId });
}

/// Para uma tarefa em andamento e devolve o estado novo dela.
export function stopBackgroundTask(id: string): Promise<BackgroundTask> {
  return invoke("stop_background_task", { id });
}

/// Tarefas em segundo plano desta sessão do app, só as da conversa.
export function listBackgroundTasks(conversationId: string): Promise<BackgroundTask[]> {
  return invoke("list_background_tasks", { conversationId });
}

/// Cada tarefa emitida quando termina.
export function onBackgroundTask(handler: (task: BackgroundTask) => void): Promise<UnlistenFn> {
  return listen<BackgroundTask>(BACKGROUND_TASK_EVENT, (e) => handler(e.payload));
}

export interface PluginScan {
  plugins: PluginManifest[];
  errors: { folder: string; message: string }[];
}

export function listPlugins(): Promise<PluginScan> {
  return invoke("list_plugins");
}

/** Arquivo de `assets/` de um plugin de tema: MIME e conteúdo em base64. */
export function readPluginAsset(id: string, path: string): Promise<{ mime: string; data: string }> {
  return invoke("read_plugin_asset", { id, path });
}

/** Encerra as execuções e os terminais em andamento, antes de instalar uma atualização. */
export function stopAllForUpdate(): Promise<void> {
  return invoke("stop_all_for_update");
}

export function readPluginEntry(id: string): Promise<string> {
  return invoke("read_plugin_entry", { id });
}

/** Plugin do Claude Code instalado, como a CLI lista (`claude plugin list --json`). */
export interface ClaudeInstalledPlugin {
  id: string;
  version: string;
  scope: string;
  enabled: boolean;
  installPath?: string | null;
}

/** Plugin que um marketplace oferece e que ainda não está instalado. */
export interface ClaudeAvailablePlugin {
  pluginId: string;
  name: string;
  description: string;
}

export interface ClaudeCatalog {
  installed: ClaudeInstalledPlugin[];
  available: ClaudeAvailablePlugin[];
}

export interface ClaudeMarketplace {
  name: string;
  origin: string;
}

/** A CLI do Claude instalada aceita os comandos de plugin que a Luna usa? */
export function claudeCliVersionOk(): Promise<boolean> {
  return invoke("claude_cli_version_ok");
}

export function claudePluginsCatalog(): Promise<ClaudeCatalog> {
  return invoke("claude_plugins_catalog");
}

/** Texto da CLI sobre o plugin: o que ele adiciona e o custo estimado. Mostrado antes de instalar. */
export function claudePluginDetails(pluginId: string): Promise<string> {
  return invoke("claude_plugin_details", { pluginId });
}

/** Instala no escopo do usuário. A Luna só chama com `confirmed` depois da confirmação da pessoa. */
export function claudePluginInstall(pluginId: string, confirmed: boolean): Promise<void> {
  return invoke("claude_plugin_install", { pluginId, confirmed });
}

export function claudePluginUninstall(pluginId: string, confirmed: boolean): Promise<void> {
  return invoke("claude_plugin_uninstall", { pluginId, confirmed });
}

export function claudePluginSetEnabled(pluginId: string, enabled: boolean): Promise<void> {
  return invoke("claude_plugin_set_enabled", { pluginId, enabled });
}

export function claudePluginUpdate(pluginId: string): Promise<void> {
  return invoke("claude_plugin_update", { pluginId });
}

export function claudeMarketplacesList(): Promise<ClaudeMarketplace[]> {
  return invoke("claude_marketplaces_list");
}

/** Origem: `dono/repo` do GitHub ou endereço `https://`. */
export function claudeMarketplaceAdd(source: string): Promise<void> {
  return invoke("claude_marketplace_add", { source });
}

/** Remover um marketplace desinstala os plugins dele, por isso exige confirmação. */
export function claudeMarketplaceRemove(name: string, confirmed: boolean): Promise<void> {
  return invoke("claude_marketplace_remove", { name, confirmed });
}

/// Modelos e níveis de esforço que a CLI do provedor aceita.
export function listModels(provider: ProviderKind): Promise<ModelOption[]> {
  return invoke("list_models", { provider });
}

/// `true` quando o CLI do provedor já tem uma conta conectada.
export function checkAuth(provider: ProviderKind): Promise<boolean> {
  return invoke("check_auth", { provider });
}

/// Abre a janela de login oficial do CLI (interativa, no navegador).
export function openLogin(provider: ProviderKind): Promise<void> {
  return invoke("open_login", { provider });
}

export function listConversations(): Promise<ConversationSummary[]> {
  return invoke("list_conversations");
}

export function loadConversation(id: string): Promise<Conversation> {
  return invoke("load_conversation", { id });
}

export function saveConversation(conversation: Conversation): Promise<void> {
  return invoke("save_conversation", { conversation });
}

export function deleteConversation(id: string): Promise<void> {
  return invoke("delete_conversation", { id });
}

export function renameConversation(id: string, title: string): Promise<void> {
  return invoke("rename_conversation", { id, title });
}

/// Situação das CLIs (Claude Code e Codex) no computador.
/// Itens de uma pasta do projeto ("" = raiz). Só leitura.
export function listProjectDir(cwd: string, path: string): Promise<ProjectEntry[]> {
  return invoke("list_project_dir", { cwd, path });
}

/// Texto de um arquivo do projeto, para o visualizador. Só leitura.
export function readProjectFile(cwd: string, path: string): Promise<string> {
  return invoke("read_project_file", { cwd, path });
}

/// Grava o texto de um arquivo que já existe no projeto (o editor do painel de arquivos).
export function writeProjectFile(cwd: string, path: string, content: string): Promise<void> {
  return invoke("write_project_file", { cwd, path, content });
}

/// Arquivos do projeto que as CLIs leem (CLAUDE.md, AGENTS.md...). Só confere; não cria nada.
export function projectCheck(cwd: string): Promise<ProjectFile[]> {
  return invoke("project_check", { cwd });
}

export function cliStatus(): Promise<CliStatus[]> {
  return invoke("cli_status");
}

/// Roda o instalador oficial da CLI. Só instala se ela ainda não estiver no computador.
export function installCli(kind: ProviderKind): Promise<CliStatus> {
  return invoke("install_cli", { kind });
}

/** Versão instalada e a mais nova de cada CLI. Só consulta; não atualiza nada. */
export function cliUpdateCheck(): Promise<CliUpdateInfo[]> {
  return invoke("cli_update_check");
}

/** Atualiza uma CLI. O Rust recusa sem `confirmed` e com uma resposta do agente em andamento. */
export function cliUpdateRun(kind: ProviderKind, confirmed: boolean): Promise<CliUpdateInfo> {
  return invoke("cli_update_run", { kind, confirmed });
}

/// Pasta de onde o `luna` foi rodado; é o ponto de partida do workspace.
export function getWorkspace(): Promise<string> {
  return invoke("get_workspace");
}

/// Abre o seletor de pastas do sistema. `null` quando o usuário cancela.
export async function pickWorkspace(current?: string): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false, defaultPath: current || undefined });
  return typeof picked === "string" ? picked : null;
}

/// Arquivos alterados na pasta de trabalho, pelo git.
export function getChanges(cwd: string): Promise<Changes> {
  return invoke("get_changes", { cwd });
}

/// Diff de um arquivo alterado.
export function getFileDiff(cwd: string, path: ChangedFile["path"]): Promise<string> {
  return invoke("get_file_diff", { cwd, path });
}

/// Lê uma imagem já salva na pasta de anexos, pelo caminho guardado na mensagem. Só essa pasta é aceita.
export function readAttachment(path: string): Promise<{ mime: string; data: string }> {
  return invoke<{ mime: string; data: string }>("read_attachment", { path });
}

/// Salva uma imagem anexada (base64) na pasta de anexos do app e devolve o caminho.
export function saveAttachment(name: string, data: string): Promise<string> {
  return invoke("save_attachment", { name, data });
}

/// Salva um arquivo qualquer anexado (base64) na pasta de anexos do app e devolve o caminho. O agente lê pelo caminho.
export function saveFileAttachment(name: string, data: string): Promise<string> {
  return invoke("save_file_attachment", { name, data });
}

/// Arquivos do projeto para o @arquivo, filtrados pelo trecho digitado (até 50).
export function listFiles(cwd: string, query: string): Promise<string[]> {
  return invoke("list_files", { cwd, query });
}

/// Skills e comandos que a CLI conhece, para o menu de `/`. Lê as pastas do usuário e, no Claude, a do projeto.
/// Regras globais: o texto fica no app e vai para ~/.claude/CLAUDE.md e ~/.codex/AGENTS.md (ver src-tauri/src/rules.rs).
export function getRules(): Promise<string> {
  return invoke<string>("get_rules");
}

/// Salva as regras e devolve os arquivos das CLIs que foram gravados.
export function saveRules(text: string): Promise<string[]> {
  return invoke<string[]>("save_rules", { text });
}

/// Bloco atual do ~/.claude/CLAUDE.md (ou o arquivo inteiro), para a primeira colagem.
export function importRules(): Promise<string> {
  return invoke<string>("import_rules");
}

export async function listSlashItems(cwd: string, provider: ProviderKind): Promise<SlashItem[]> {
  const items = await invoke<{ name: string; description: string; kind: "skill" | "command" | "plugin-skill" | "plugin-command" }[]>(
    "list_slash_items",
    { cwd, provider },
  );
  const origin = { skill: "skill", command: "command", "plugin-skill": "claude-skill", "plugin-command": "claude-command" } as const;
  return items.map((item) => ({ name: item.name, description: item.description, origin: origin[item.kind] }));
}

/// Grava o estado dos arquivos antes de um turno. `null` fora de repositório git.
export function checkpointTake(cwd: string): Promise<string | null> {
  return invoke("checkpoint_take", { cwd });
}

export interface RestorePreview {
  /** Arquivos criados depois do checkpoint: serão apagados. */
  removed: string[];
  /** Arquivos alterados ou removidos depois: voltam ao que eram. */
  restored: string[];
}

export function checkpointPreview(cwd: string, checkpoint: string): Promise<RestorePreview> {
  return invoke("checkpoint_preview", { cwd, checkpoint });
}

export function checkpointRestore(cwd: string, checkpoint: string): Promise<RestorePreview> {
  return invoke("checkpoint_restore", { cwd, checkpoint });
}

/// Desfaz a alteração de um arquivo: volta ao HEAD, ou apaga o que nunca foi commitado.
export function discardChange(cwd: string, path: string, status: ChangedFile["status"]): Promise<void> {
  return invoke("discard_change", { cwd, path, status });
}

/// Abre um shell na pasta dada e devolve o id para escrever, redimensionar e fechar.
export function terminalOpen(cwd: string, cols: number, rows: number): Promise<string> {
  return invoke("terminal_open", { cwd, cols, rows });
}

export function terminalWrite(id: string, data: string): Promise<void> {
  return invoke("terminal_write", { id, data });
}

export function terminalResize(id: string, cols: number, rows: number): Promise<void> {
  return invoke("terminal_resize", { id, cols, rows });
}

export function terminalClose(id: string): Promise<void> {
  return invoke("terminal_close", { id });
}

/// Retângulo da página do navegador, em pixels lógicos e relativo à janela.
export interface BrowserBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

/// Uma aba do navegador: o id e o endereço da página.
export interface BrowserTab {
  id: number;
  url: string;
  /** Economia de memória ligada: a aba é suspensa quando fica escondida. */
  economize?: boolean;
}

export interface BrowserState {
  tabs: BrowserTab[];
  active: number | null;
  /** Bloqueador de anúncios ligado (padrão) ou pausado. */
  adblock: boolean;
}

/// Traduz a página da aba ativa, ou volta ao original quando `on` é falso. Devolve o endereço que a aba passou a mostrar.
export function browserTranslate(on: boolean): Promise<string> {
  return invoke("browser_translate", { on });
}

/// Liga ou pausa o bloqueador de anúncios, em todas as abas.
export function browserSetAdblock(on: boolean): Promise<void> {
  return invoke("browser_set_adblock", { on });
}

/// Abre uma aba nova (em branco sem endereço) e a torna a ativa. Devolve o id.
export function browserNewTab(url: string | null, bounds: BrowserBounds): Promise<number> {
  return invoke("browser_new_tab", { url, bounds });
}

/// Mostra a aba escolhida e esconde as outras.
export function browserSwitchTab(tabId: number, bounds: BrowserBounds): Promise<void> {
  return invoke("browser_switch_tab", { tabId, bounds });
}

/// Fecha uma aba. Devolve a aba ativa depois do fechamento, ou `null` se não sobrou nenhuma.
export function browserCloseTab(tabId: number): Promise<number | null> {
  return invoke("browser_close_tab", { tabId });
}

/// Liga ou desliga a economia de memória de uma aba. Ligada, a aba é suspensa quando fica escondida.
export function browserSetEconomy(tabId: number, economize: boolean): Promise<void> {
  return invoke("browser_set_economy", { tabId, economize });
}

/// Abas abertas e a ativa, como o Rust as guarda.
export function browserState(): Promise<BrowserState> {
  return invoke("browser_state");
}

/// Projeto que o navegador mostra: as abas dele aparecem e as dos outros projetos são escondidas.
export function browserSetProject(project: string): Promise<void> {
  return invoke("browser_set_project", { project });
}

/// Favorito do navegador: uma página salva para abrir depois.
export interface BrowserBookmark {
  id: string;
  title: string;
  url: string;
}

/// Chromium do Playwright já baixado no computador (o agente só o usa se a escolha for o Playwright).
export function playwrightStatus(): Promise<boolean> {
  return invoke("playwright_status");
}

/// Baixa o Chromium do Playwright. Só chamar depois que a pessoa confirmar.
export function playwrightInstall(): Promise<void> {
  return invoke("playwright_install");
}

/// Ícone do site como `data:` (ou null, quando o site não tem). O Rust busca, por causa da CSP da janela.
export function browserFavicon(pageUrl: string): Promise<string | null> {
  return invoke("browser_favicon", { pageUrl });
}

export function browserBookmarks(): Promise<BrowserBookmark[]> {
  return invoke("browser_bookmarks");
}

export function browserBookmarkAdd(url: string, title: string): Promise<BrowserBookmark[]> {
  return invoke("browser_bookmark_add", { url, title });
}

export function browserBookmarkRemove(id: string): Promise<BrowserBookmark[]> {
  return invoke("browser_bookmark_remove", { id });
}

export function browserSetBounds(bounds: BrowserBounds): Promise<void> {
  return invoke("browser_set_bounds", { bounds });
}

/// Mostra ou esconde a página. Sem navegador aberto, esconder não faz nada.
export function browserShow(visible: boolean): Promise<void> {
  return invoke("browser_show", { visible });
}

export function browserNavigate(url: string): Promise<string> {
  return invoke("browser_navigate", { url });
}

export function browserHistory(direction: "back" | "forward"): Promise<void> {
  return invoke("browser_history", { direction });
}

export function browserReload(): Promise<void> {
  return invoke("browser_reload");
}

/// Endereço local da prévia de uma subpasta do clone (o app serve a pasta, só para leitura).
export function browserPreviewUrl(cwd: string, folder: string): Promise<string> {
  return invoke("browser_preview_url", { cwd, folder });
}

/// Motor de busca do agente do navegador. O localStorage não chega ao Rust, então o front avisa a escolha.
export function browserSetSearchEngine(engine: SearchEngine): Promise<void> {
  return invoke("browser_set_search_engine", { engine });
}

/// Visita registrada no histórico do app (browser-visits.json, da mais recente para a mais antiga).
export interface BrowserVisit {
  url: string;
  visitedAt: number;
}

export function browserVisits(): Promise<BrowserVisit[]> {
  return invoke("browser_visits");
}

/// Papel do arquivo HTML exportado: os favoritos entram na lista ou as páginas no histórico.
export type ImportTarget = "bookmarks" | "history";

/// Quantos links saíram do arquivo para cada destino.
export interface ImportHtmlResult {
  bookmarks: number;
  visits: number;
}

export async function importBrowserHtml(path: string, target: ImportTarget): Promise<ImportHtmlResult> {
  const count = await invoke<number>("browser_import_html", { path, target });
  return target === "bookmarks" ? { bookmarks: count, visits: 0 } : { bookmarks: 0, visits: count };
}

/// Senha importada, aberta para consulta e para copiar. Nunca vai para a tela sem o clique.
export interface ImportedPassword {
  url: string;
  login: string;
  password: string;
}

export function importBrowserPasswords(path: string): Promise<number> {
  return invoke("browser_import_passwords", { path });
}

export function browserPasswords(): Promise<ImportedPassword[]> {
  return invoke("browser_passwords");
}

/// Logins salvos para a página aberta, sem senha. A origem é conferida pelo Rust.
export function browserLoginSuggestions(): Promise<string[]> {
  return invoke("browser_login_suggestions");
}

/// Preenche login e senha salvos na página aberta. A senha fica no Rust: o front só manda o login escolhido.
export function browserFillLogin(login: string): Promise<void> {
  return invoke("browser_fill_login", { login });
}

/// Resumo do site (título, cores, animações, componentes). O Rust recusa sem a confirmação de direitos.
export interface SiteCapture {
  url: string;
  title: string;
  colors: string[];
  keyframes: number;
  components: string[];
  stylesheets: number;
  /** Variáveis CSS com cor, como `--cor-primaria: #1a73e8`. */
  cssVariables: string[];
  /** CSS embutido da página, resumido em uma linha. */
  embeddedCss: string;
}

export function siteCloneCapture(url: string, rightsConfirmed: boolean): Promise<SiteCapture> {
  return invoke("site_clone_capture", { url, rightsConfirmed });
}

/// Escolhe um arquivo exportado pelo navegador: HTML de favoritos/histórico ou CSV de senhas.
export async function pickImportFile(kind: "html" | "csv"): Promise<string | null> {
  return open({
    multiple: false,
    filters:
      kind === "html"
        ? [{ name: "HTML exportado", extensions: ["html", "htm"] }]
        : [{ name: "CSV exportado", extensions: ["csv"] }],
  });
}

export interface TerminalDataEvent {
  id: string;
  data: string;
}

/// Saída de um terminal. Cada handler filtra pelo próprio id.
export function onTerminalData(handler: (event: TerminalDataEvent) => void): Promise<UnlistenFn> {
  return listen<TerminalDataEvent>("terminal-data", (e) => handler(e.payload));
}

/** Página aberta pelo agente (ferramenta do navegador); o endereço chega como texto. */
export function onBrowserOpened(handler: (url: string) => void): Promise<UnlistenFn> {
  return listen<string>("browser://opened", (e) => handler(e.payload));
}

/// Atalho (Ctrl+T, Ctrl+W, Ctrl+L ou Ctrl+F) pressionado com o foco numa página do navegador. A tecla vem em minúscula.
export function onBrowserShortcut(handler: (key: string) => void): Promise<UnlistenFn> {
  return listen<string>("browser://shortcut", (e) => handler(e.payload));
}

/// A lista de favoritos mudou (salvou, removeu ou importou): as telas relêem a lista.
export function onBrowserBookmarksChanged(handler: () => void): Promise<UnlistenFn> {
  return listen("browser://bookmarks-changed", () => handler());
}

/// Uma página terminou de carregar numa aba, inclusive depois de um clique num link.
export function onBrowserNavigated(handler: () => void): Promise<UnlistenFn> {
  return listen("browser://navigated", () => handler());
}

export function onTerminalExit(handler: (id: string) => void): Promise<UnlistenFn> {
  return listen<{ id: string }>("terminal-exit", (e) => handler(e.payload.id));
}

/// Provedores prontos para criar um perfil de API.
export function apiPresets(): Promise<ApiPreset[]> {
  return invoke("api_presets");
}

export function listApiProfiles(): Promise<ApiProfile[]> {
  return invoke("list_api_profiles");
}

/// Salva o perfil sem a chave e devolve o perfil com o id definido.
export function saveApiProfile(profile: Omit<ApiProfile, "id"> & { id?: string }): Promise<ApiProfile> {
  return invoke("save_api_profile", { profile: { id: "", ...profile } });
}

export function deleteApiProfile(id: string): Promise<void> {
  return invoke("delete_api_profile", { id });
}

/// Guarda a chave no cofre do Windows. Não há função para ler a chave de volta.
export function setApiProfileKey(id: string, key: string): Promise<void> {
  return invoke("set_api_profile_key", { id, key });
}

export function apiProfileHasKey(id: string): Promise<boolean> {
  return invoke("api_profile_has_key", { id });
}

/// Manda um prompt mínimo pelo perfil e devolve a resposta do modelo.
export function testApiProfile(id: string): Promise<string> {
  return invoke("test_api_profile", { id });
}

/// Busca a lista de modelos do perfil salvo na API. Não grava nada: a tela decide.
export function refreshApiModels(id: string): Promise<RemoteModels> {
  return invoke("refresh_api_models", { id });
}

/// Busca a lista de modelos com a chave que está na tela, antes de salvar. A chave não é gravada.
export function listProviderModels(preset: string, baseUrl: string, apiKey: string): Promise<RemoteModels> {
  return invoke("list_provider_models", { preset, baseUrl, apiKey });
}

/// Avisa as telas que a lista de perfis mudou (ver ProvidersSection e ChatView).
export const PROFILES_CHANGED = "luna:profiles-changed";
/// Avisa o chat que a lista de plugins mudou (instalar, desinstalar, ligar ou desligar).
export const PLUGINS_CHANGED = "luna:plugins-changed";

/// Nome do catálogo oficial embutido (ver src-tauri/marketplace/official.json).
export const OFFICIAL_SOURCE = "oficial";

export interface CatalogFile {
  path: string;
  sha256: string;
  content: string | null;
  url: string | null;
}

export interface CatalogPlugin {
  id: string;
  name: string;
  description: string;
  author: string;
  version: string;
  entry: string;
  permissions: string[];
  files: CatalogFile[];
}

export interface Catalog {
  name: string;
  plugins: CatalogPlugin[];
}

export interface PluginsState {
  disabled: string[];
}

export function listCatalogSources(): Promise<string[]> {
  return invoke("list_catalog_sources");
}

/// Baixa e confere o catálogo antes de guardar a URL.
export function addCatalogSource(url: string): Promise<Catalog> {
  return invoke("add_catalog_source", { url });
}

export function removeCatalogSource(url: string): Promise<void> {
  return invoke("remove_catalog_source", { url });
}

/// `source` é `OFFICIAL_SOURCE` ou uma URL já adicionada.
export function loadCatalog(source: string): Promise<Catalog> {
  return invoke("load_catalog", { source });
}

export function installCatalogPlugin(source: string, id: string): Promise<void> {
  return invoke("install_catalog_plugin", { source, id });
}

export function uninstallPlugin(id: string): Promise<void> {
  return invoke("uninstall_plugin", { id });
}

/// Abre o seletor de pastas para escolher um plugin no computador. `null` quando cancela.
export async function pickPluginFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === "string" ? picked : null;
}

/// Lê o manifesto de uma pasta escolhida, sem instalar: para mostrar as permissões antes.
export function inspectLocalPlugin(folder: string): Promise<PluginManifest> {
  return invoke("inspect_local_plugin", { folder });
}

/// Instala um plugin de uma pasta escolhida no computador.
export function installLocalPlugin(folder: string): Promise<PluginManifest> {
  return invoke("install_local_plugin", { folder });
}

/// Abre a pasta de plugins no gerenciador de arquivos do sistema.
export function openPluginsFolder(): Promise<void> {
  return invoke("open_plugins_folder");
}

/// Cria um plugin novo com o modelo na pasta de plugins. Devolve a pasta criada.
export function createPlugin(id: string, name: string): Promise<string> {
  return invoke("create_plugin", { id, name });
}

export function getPluginsState(): Promise<PluginsState> {
  return invoke("get_plugins_state");
}

export function setPluginEnabled(id: string, enabled: boolean): Promise<PluginsState> {
  return invoke("set_plugin_enabled", { id, enabled });
}

/// Skill instalada nas CLIs (ver src-tauri/src/skills.rs).
export interface InstalledSkill {
  id: string;
  name: string;
  description: string;
  /** "claude" e/ou "codex": onde a pasta está instalada. */
  engines: string[];
}

export interface SkillCatalogEntry {
  id: string;
  name: string;
  description: string;
  author: string;
  version: string;
}

/// Branches locais do repositório da pasta. Pasta sem git dá erro: a tela trata como lista vazia.
export function listGitBranches(cwd: string): Promise<string[]> {
  return invoke<string[]>("list_git_branches", { cwd });
}

/// Pasta do app para conversas sem projeto. Cria a pasta se ainda não existir.
export function freeWorkspace(): Promise<string> {
  return invoke<string>("free_workspace");
}

/// Branch em que a pasta está agora (null quando a HEAD está solta ou a pasta não é repositório).
export function currentGitBranch(cwd: string): Promise<string | null> {
  return invoke<string | null>("current_git_branch", { cwd });
}

/// Cria a worktree numa branch existente (ou numa nova, a partir da atual) e devolve a pasta dela.
export function createGitWorktree(cwd: string, branch: string, newBranch: boolean): Promise<string> {
  return invoke<string>("create_git_worktree", { cwd, branch, newBranch });
}

/// Impressão digital da configuração (MCP, skills e plugins). Muda quando a Luna instala algo pelo chat.
export function configFingerprint(): Promise<string> {
  return invoke<string>("config_fingerprint");
}

export const SKILLS_CHANGED = "luna:skills-changed";

export function listSkills(): Promise<InstalledSkill[]> {
  return invoke<InstalledSkill[]>("list_skills");
}

/// Catálogo oficial de skills. Os arquivos ficam no app e não entram na resposta.
/// `source` vazio = catálogo oficial; senão a URL de um catálogo adicionado.
export function loadSkillCatalog(source?: string): Promise<{ skills: SkillCatalogEntry[] }> {
  return invoke<{ skills: SkillCatalogEntry[] }>("load_skill_catalog", { source: source ?? null });
}

export function installSkill(id: string, source?: string): Promise<string[]> {
  return invoke<string[]>("install_skill", { id, source: source ?? null });
}

export function listSkillSources(): Promise<string[]> {
  return invoke<string[]>("list_skill_sources");
}

export function addSkillSource(url: string): Promise<string[]> {
  return invoke<string[]>("add_skill_source", { url });
}

export function removeSkillSource(url: string): Promise<string[]> {
  return invoke<string[]>("remove_skill_source", { url });
}

export function uninstallSkill(id: string): Promise<void> {
  return invoke<void>("uninstall_skill", { id });
}

/// Servidor MCP gravado nas CLIs (ver src-tauri/src/mcp.rs).
export interface McpServer {
  name: string;
  transport: "stdio" | "http";
  command?: string;
  args: string[];
  url?: string;
  /** Nomes das variáveis que o servidor lê do ambiente do usuário. Os valores nunca chegam ao app. */
  envVars: string[];
  /** Variáveis com valor escrito nos arquivos (formato antigo). Só o nome; o botão Reconfigurar troca. */
  literalEnv: string[];
  /** "claude" e/ou "codex"; vazio = desligado. */
  engines: string[];
  enabled: boolean;
}

export interface McpCatalogEntry {
  id: string;
  name: string;
  description: string;
  author: string;
  transport: "stdio" | "http";
  command?: string;
  args: string[];
  url?: string;
  env: { key: string; label: string }[];
}

export const MCP_CHANGED = "luna:mcp-changed";

export function listMcpServers(): Promise<McpServer[]> {
  return invoke<McpServer[]>("list_mcp_servers");
}

export function setMcpEnabled(name: string, enabled: boolean): Promise<void> {
  return invoke<void>("set_mcp_enabled", { name, enabled });
}

export function removeMcpServer(name: string): Promise<void> {
  return invoke<void>("remove_mcp_server", { name });
}

export function loadMcpCatalog(source?: string): Promise<{ servers: McpCatalogEntry[] }> {
  return invoke<{ servers: McpCatalogEntry[] }>("load_mcp_catalog", { source: source ?? null });
}

/// Instala do catálogo. Não recebe valores: a variável é definida no ambiente do usuário.
export function installMcpFromCatalog(id: string, source?: string): Promise<void> {
  return invoke<void>("install_mcp_from_catalog", { id, source: source ?? null });
}

/// Troca o valor escrito no arquivo por referência ao nome da variável (ver mcp.rs).
export function reconfigureMcpServer(name: string): Promise<void> {
  return invoke<void>("reconfigure_mcp_server", { name });
}

export function listMcpSources(): Promise<string[]> {
  return invoke<string[]>("list_mcp_sources");
}

export function addMcpSource(url: string): Promise<string[]> {
  return invoke<string[]>("add_mcp_source", { url });
}

export function removeMcpSource(url: string): Promise<string[]> {
  return invoke<string[]>("remove_mcp_source", { url });
}

/// Grava um servidor escrito à mão, nas duas CLIs. Só os nomes das variáveis entram.
export function saveMcpServer(server: Pick<McpServer, "name" | "transport" | "command" | "args" | "url" | "envVars">): Promise<void> {
  return invoke<void>("save_mcp_server", { server: { ...server, literalEnv: [], engines: [], enabled: true } });
}
