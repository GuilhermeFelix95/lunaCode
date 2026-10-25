import { useCallback, useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import { ArrowDown, ArrowLeft, GitCompare, Globe, PanelRightClose, PanelRightOpen, Terminal } from "lucide-react";
import {
  cancelPrompt,
  checkAuth,
  getChanges,
  getWorkspace,
  projectCheck,
  apiPresets,
  checkpointPreview,
  checkpointRestore,
  checkpointTake,
  apiProfileHasKey,
  listApiProfiles,
  listFiles,
  listModels,
  listSlashItems,
  saveAttachment,
  saveFileAttachment,
  onProviderEvent,
  onPermissionRequest,
  answerPermission,
  type PermissionRequest,
  PLUGINS_CHANGED,
  PROFILES_CHANGED,
  openLogin,
  pickWorkspace,
  sendPrompt,
  listAccounts,
  ACCOUNTS_CHANGED,
  createSpec,
  recordSpecDecisions,
  getOrchestrator,
  currentGitBranch,
  freeWorkspace,
  listGitBranches,
  createGitWorktree,
  configFingerprint,
  MCP_CHANGED,
  SKILLS_CHANGED,
  listBackgroundTasks,
  onBackgroundTask,
  startBackgroundTask,
  stopBackgroundTask,
} from "../../api";
import { agentKey, agentOptions } from "../../chat/agents";
import { routeFor, type Orchestrator, type OrchestratorRole } from "../../chat/orchestrator";
import { getAgentBrowser } from "../../chat/agentBrowser";
import { getBrowserAgent } from "../../chat/browserAgent";
import { profileModelOptions } from "../../chat/models";
import { contextFor, withContext } from "../../chat/handoff";
import { autoCompactEnabled, compactPreamble, COMPACT_PROMPT, needsCompaction, sessionsFrom } from "../../chat/compact";
import { goalInstruction, goalShouldContinue, isGoalDone, MAX_GOAL_ROUNDS, stripGoalMarker } from "../../chat/goal";
import { tokensPerSecond } from "../../chat/speed";
import { APPROVE_OPTION, approvalPrompt, contextUnderstood, extractPlan, loadPlanTarget, planFromReply, planIsReady, QUESTION_PROTOCOL, streamingPlan, withPlanInstruction } from "../../chat/planning";
import { extractQuestions, turnAsksQuestions, type Question, type ReplyParts } from "../../chat/questions";
import { applyEvent } from "../../chat/reducer";
import { summarize, type CacheUsage, type TurnRecord } from "../../chat/promptCache";
import { longCacheEnabled } from "../../chat/cacheSettings";
import { useStickToBottom } from "../../chat/useStickToBottom";
import { createFrameBatch } from "../../chat/frameBatch";
import { AUTOFIX_MAX_ATTEMPTS, PluginHost, type PluginMode } from "../../plugins/host";
import { notifyIfUnfocused } from "../../notifications";
import { FilesPanel } from "../side/FilesPanel";
import { FileEditor } from "../side/FileEditor";
import { cliPrompt, filterSlash, LUNA_COMMANDS, parseLunaCommand, slashQuery, type SlashItem } from "../../chat/slash";
import { extractPluginOffer } from "../../chat/pluginOffer";
import { PluginOfferCard } from "./PluginOfferCard";
import { playReplySound } from "../../sounds";
import { showToast } from "../../ui/toast";
import { loadInstalledPlugins } from "../../plugins/loader";
import { setPluginThemes } from "../../plugins/themeRegistry";
import type {
  AgentSessions,
  ApiPreset,
  ApiProfile,
  Account,
  BackgroundTask,
  ChatMessage,
  CliUpdateInfo,
  Compaction,
  Conversation,
  ModelOption,
  Permission,
  ProviderKind,
} from "../../types";
import { updatesToShow } from "../../chat/cliUpdates";
import { CliUpdateNotice } from "./CliUpdateNotice";
import { applyTaskEvent, displayTodos, toTodos, type TaskEntry } from "../../chat/tasks";
import { ChangesPanel } from "../changes/ChangesPanel";
import { LoginPrompt } from "../auth/LoginPrompt";
import { BrowserPanel } from "../side/BrowserPanel";
import { RightPanel, type SideTab } from "../side/RightPanel";
import { TerminalTabs } from "../terminal/TerminalTabs";
import { Composer, PROVIDER_LABELS, type ComposerHandle } from "./Composer";
import { PermissionPrompt } from "./PermissionPrompt";
import { MessageList } from "./MessageList";
import { QuestionPicker } from "./QuestionPicker";
import { SiteCloneFlow } from "./SiteCloneFlow";
import { SiteClonePreview } from "./SiteClonePreview";
import { UsageMeter, type LimitState, type ContextState } from "./UsageMeter";
import { WorkspaceChip } from "./WorkspaceChip";
import { BackgroundTasks } from "./BackgroundTasks";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { saveLastWorkspace } from "../../workspace";
import { t, useLocale, type MessageKey } from "../../i18n";

const TITLE_MAX_CHARS = 60;

/// Sugestões da tela vazia: preenchem a caixa de mensagem, sem enviar sozinhas.
const SUGGESTIONS: MessageKey[] = ["chat.suggestion.explain", "chat.suggestion.bugs", "chat.suggestion.tests"];

interface ChatViewProps {
  /** Conversa aberta do histórico. Ausente = conversa nova. */
  initial?: Conversation;
  /** Pasta de uma conversa nova. Ausente = pasta de onde o luna foi rodado. */
  startCwd?: string;
  /** Pastas de conversas anteriores, para o menu de troca. */
  recentFolders?: string[];
  /** Chamado ao fim de cada turno, com a conversa inteira, para salvar em disco. */
  onPersist: (conversation: Conversation) => void;
  /** Avisa a barra lateral quando esta conversa começa ou termina uma resposta. */
  onRunning?: (conversationId: string, running: boolean) => void;
  /** Esta conversa é a que está na tela. Conversas ocultas não mostram a página do navegador. */
  visible?: boolean;
  /** Modo navegador: o navegador ocupa a tela e este chat vira a sidebar lateral. */
  browserMode?: boolean;
  /** Volta ao modo LunaCode normal, pelo botão da topbar do navegador. */
  onExitBrowserMode?: () => void;
  /** Abre outra sessão do navegador, sem apagar a atual (o histórico fica acessível no modo navegador). */
  onNewSession?: () => void;
  /** Situação das CLIs (ver `cli_update_check`). O aviso só aparece com o agente parado. */
  cliUpdates?: CliUpdateInfo[];
  /** Uma CLI foi atualizada: quem guarda a lista troca a linha dela. */
  onCliUpdated?: (info: CliUpdateInfo) => void;
}

/// Largura da sidebar do agente no modo navegador: entre 300 px e 60% da janela.
const AGENT_WIDTH_KEY = "luna.browserAgent.width";
const DEFAULT_AGENT_WIDTH = 340;
const MIN_AGENT_WIDTH = 300;
const MAX_AGENT_FRACTION = 0.6;
const AGENT_KEY_STEP = 16;

function clampAgentWidth(width: number, windowWidth: number): number {
  const max = Math.max(MIN_AGENT_WIDTH, Math.floor(windowWidth * MAX_AGENT_FRACTION));
  return Math.min(Math.max(Math.round(width), MIN_AGENT_WIDTH), max);
}

function loadAgentWidth(): number {
  try {
    const saved = Number(localStorage.getItem(AGENT_WIDTH_KEY));
    if (Number.isFinite(saved) && saved > 0) return clampAgentWidth(saved, window.innerWidth);
  } catch {
    // Armazenamento indisponível: usa o padrão.
  }
  return DEFAULT_AGENT_WIDTH;
}

function saveAgentWidth(width: number) {
  try {
    localStorage.setItem(AGENT_WIDTH_KEY, String(width));
  } catch {
    // Sem armazenamento, a largura vale só nesta sessão.
  }
}

/// Escolha de modelo por papel, lida a cada envio: o que a pessoa salvou em Configurações vale na próxima execução.
/// Sem a escolha (ou sem o comando, em testes), vale a conversa atual.
async function loadOrchestrator(): Promise<Orchestrator> {
  try {
    return (await getOrchestrator()) ?? {};
  } catch {
    return {};
  }
}

export function ChatView({
  initial,
  startCwd,
  recentFolders = [],
  onPersist,
  onRunning,
  visible = true,
  browserMode = false,
  onExitBrowserMode,
  onNewSession,
  cliUpdates = [],
  onCliUpdated = () => {},
}: ChatViewProps) {
  useLocale();
  const [conversationId] = useState(() => initial?.id ?? crypto.randomUUID());
  const createdAt = useRef(initial?.createdAt ?? Date.now());

  const [provider, setProvider] = useState<ProviderKind>(initial?.provider ?? "claude");
  // Perfil de API escolhido (null = login da CLI). O agente da conversa é o motor mais o perfil.
  const [profileId, setProfileId] = useState<string | null>(initial?.profile ?? null);
  const [profiles, setProfiles] = useState<ApiProfile[]>([]);
  // Conta extra escolhida no composer (id em accounts.json). Vazio = login padrão da CLI.
  const [account, setAccount] = useState(initial?.account ?? "");
  const accountRef = useRef(account);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [presets, setPresets] = useState<ApiPreset[]>([]);
  const [costUsd, setCostUsd] = useState(initial?.costUsd ?? 0);
  const costRef = useRef(costUsd);
  const [permission, setPermission] = useState<Permission>(initial?.permission ?? "fullAccess");
  // Fluxo do clonador de sites aberto acima da caixa de mensagem, e a pasta do último clone para a prévia.
  const [cloning, setCloning] = useState(false);
  const [clonePreview, setClonePreview] = useState<string | null>(null);
  const [cwd, setCwd] = useState(initial?.cwd ?? startCwd ?? "");
  const [model, setModel] = useState(initial?.model ?? "");
  const [effort, setEffort] = useState(initial?.effort ?? "");
  // Modo plano começa desligado: só liga com o clique em Planejar.
  const [planning, setPlanning] = useState((initial?.planning ?? false) && !browserMode);
  // Modo navegador desliga o plano mesmo com o chat já montado.
  useEffect(() => {
    if (browserMode) setPlanning(false);
  }, [browserMode]);
  // Último plano da conversa: a resposta do modo plano (ou da spec) fica no painel.
  const [plan, setPlan] = useState<string | undefined>(initial?.plan);
  // O último turno no modo plano trouxe o plano pronto, e não só uma rodada de perguntas: só então aparece o card de aprovação.
  const [planReady, setPlanReady] = useState(Boolean(initial?.plan));
  const planRef = useRef(plan);
  // Pasta da spec desta conversa (specs/NNN-slug), criada na primeira aprovação do plano.
  const specDirRef = useRef<string | undefined>(initial?.specDir);
  // Rodadas de perguntas do modo plano feitas antes da spec existir: gravadas na aprovação.
  const pendingRounds = useRef<string[][]>([]);
  // O turno em andamento é de planejamento? Só a resposta desse turno vira plano.
  const turnPlanning = useRef(false);
  // Só turnos de planejamento e do modo Conversar mostram perguntas no seletor; nos outros, ficam como texto.
  const turnAsk = useRef(false);
  // Chave do cache para o turno que está para começar, e a do turno em andamento (null = não guarda).
  // Cache de prompt: o uso do turno chega antes do done; o livro guarda os turnos da conversa.
  const turnUsage = useRef<CacheUsage | null>(null);
  // Pedidos de permissão do Claude esperando a pessoa, e as ferramentas liberadas com "Sempre nesta conversa".
  const [permissionRequests, setPermissionRequests] = useState<PermissionRequest[]>([]);
  const alwaysAllowed = useRef(new Set<string>());
  const turnMeta = useRef({ provider: "claude" as ProviderKind, model: "", systemKey: "" });
  const cacheTurns = useRef<TurnRecord[]>([]);
  const [models, setModels] = useState<ModelOption[]>([]);
  const [messages, setMessages] = useState<ChatMessage[]>(initial?.messages ?? []);
  const messagesRef = useRef(messages);
  // A resposta chega em pedaços: a tela troca de mensagens no máximo uma vez por quadro (ver chat/frameBatch.ts).
  const [frame] = useState(() => createFrameBatch(() => setMessages(messagesRef.current)));
  const live = useRef({ provider, profileId, cwd, permission, model, effort, planning, running: false, sideTab: "changes" as SideTab });
  // O texto do campo mora no Composer, para cada tecla não re-renderizar a conversa inteira.
  // A conversa guarda uma cópia (para enviar) e o trecho do "/" (para o menu de comandos).
  const draftRef = useRef("");
  const composerRef = useRef<ComposerHandle>(null);
  const [slashText, setSlashText] = useState<string | null>(null);
  /// Põe um texto no campo: sugestões, comandos e arquivos escolhidos, e a volta no tempo.
  function setDraft(value: string) {
    draftRef.current = value;
    setSlashText(slashQuery(value));
    composerRef.current?.setValue(value);
  }
  /// Pede à conversa um resumo. Quando ele chega, a próxima mensagem começa uma sessão nova com ele.
  function compact() {
    if (running || messagesRef.current.length === 0) return;
    compactPending.current = true;
    void run(t("chat.compactShown"), COMPACT_PROMPT, false);
  }

  /// Pasta de um plugin à espera de instalação: o cartão aparece acima do composer.
  const [pluginOffer, setPluginOffer] = useState<string | undefined>(undefined);
  // Sugestões do @arquivo para o trecho digitado depois do último "@".
  const [files, setFiles] = useState<string[]>([]);
  const fileRequest = useRef(0);
  // Arquivos escolhidos no @ desde o último envio: o pedido recebe a lista deles.
  const picked = useRef(new Set<string>());
  // Imagens do próximo envio: o arquivo já está na pasta de anexos; a url é só a miniatura.
  const [attachments, setAttachments] = useState<{ id: number; path: string; url: string }[]>([]);
  const attachmentSeq = useRef(0);
  // Arquivos gerais (não imagem) do próximo envio: o caminho entra no pedido e o agente lê.
  const [fileAttachments, setFileAttachments] = useState<{ id: number; name: string; path: string }[]>([]);
  const [running, setRunning] = useState(false);
  // Tarefas em segundo plano desta sessão do app: a lista aparece acima do composer.
  const [backgroundTasks, setBackgroundTasks] = useState<BackgroundTask[]>([]);
  // Resultados já anexados ao histórico: o evento de conclusão chega uma vez, mas o app pode repetir.
  const shownBackgroundResults = useRef(new Set<string>());

  // Lista as tarefas desta sessão do app e acompanha o fim delas: o resultado entra na conversa,
  // sem mexer no turno em andamento (o processo da tarefa é independente).
  useEffect(() => {
    let alive = true;
    listBackgroundTasks(conversationId).then(
      (list) => {
        if (alive) setBackgroundTasks(list.filter((task) => !task.conversationId || task.conversationId === conversationId));
      },
      () => {},
    );
    const unlisten = onBackgroundTask((task) => {
      // O evento é do app inteiro: a tarefa de outra conversa não entra nesta.
      if (task.conversationId && task.conversationId !== conversationId) return;
      setBackgroundTasks((current) => {
        const index = current.findIndex((t) => t.id === task.id);
        if (index < 0) return [...current, task];
        const next = [...current];
        next[index] = task;
        return next;
      });
      if (task.status !== "done" && task.status !== "failed") return;
      if (shownBackgroundResults.current.has(task.id)) return;
      shownBackgroundResults.current.add(task.id);
      const header =
        task.status === "done"
          ? t("chat.backgroundResult", { prompt: task.prompt })
          : t("chat.backgroundFailure", { prompt: task.prompt });
      const output = task.output.trim() || t("chat.backgroundNoOutput");
      commit([
        ...messagesRef.current,
        { role: "assistant", text: `${header}\n\n${output}`, at: Date.now(), provider: "claude", background: true },
      ]);
      persist(messagesRef.current);
    });
    return () => {
      alive = false;
      unlisten.then((stop) => stop());
    };
    // commit e persist só usam refs e setters; a conversa aberta é o que muda a assinatura.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [conversationId]);

  /// Para uma tarefa em segundo plano e atualiza a linha da lista com o estado devolvido.
  async function stopBackground(id: string) {
    try {
      const stopped = await stopBackgroundTask(id);
      setBackgroundTasks((current) =>
        current.map((task) => (task.id === stopped.id ? { ...task, ...stopped, prompt: stopped.prompt || task.prompt } : task)),
      );
    } catch (err) {
      setNotes((current) => [...current, t("chat.backgroundStopFailed", { error: String(err) })]);
    }
  }
  // Início do turno em andamento: o cabeçalho do trabalho mostra o tempo decorrido.
  const [turnStartedAt, setTurnStartedAt] = useState<number | null>(null);
  // Contexto cheio depois de um turno: compacta antes de o usuário precisar pedir.
  useEffect(() => {
    if (running || !compactDue.current) return;
    compactDue.current = false;
    compact();
    // compact só lê o estado da renderização atual; roda quando o turno termina.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [running]);
  const [fixRound, setFixRound] = useState(0);
  // Correção do LunaFix: roda depois da renderização do turno, com a conversa atual.
  useEffect(() => {
    const prompt = fixDue.current;
    if (!prompt || fixRound === 0) return;
    fixDue.current = null;
    void run(t("autofix.shown"), prompt, false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fixRound]);
  // Modo /meta: a rodada seguinte roda depois da renderização do turno, como a correção do LunaFix.
  const [goalRound, setGoalRound] = useState(0);
  useEffect(() => {
    const next = goalDue.current;
    if (!next || goalRound === 0) return;
    goalDue.current = null;
    void run(next.shown, next.prompt, false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [goalRound]);
  // Perguntas do agente, mostradas no seletor. `questionsRound` muda a cada conjunto novo e recria o seletor.
  const [questions, setQuestions] = useState<Question[] | null>(null);
  const [questionsRound, setQuestionsRound] = useState(0);
  /// Quanto do contexto o agente diz já ter, lido da última resposta do modo plano ("Contexto entendido: X%").
  const [understood, setUnderstood] = useState<number | null>(null);
  // MCP, skill ou plugin instalado pela Luna no turno: a tela oferece recarregar. A impressão digital é tirada antes e depois do turno.
  const configBefore = useRef<string | null>(null);
  const [configChanged, setConfigChanged] = useState(false);
  // Branches da pasta, para o menu de pasta. Fora de um repositório git a lista fica vazia.
  const [branches, setBranches] = useState<string[]>([]);
  const [currentBranch, setCurrentBranch] = useState<string | null>(null);
  // Sobe a cada comando concluído, a cada fim de turno e a cada abertura do menu: o agente pode ter criado ou trocado de branch.
  const [branchesVersion, setBranchesVersion] = useState(0);
  // Pasta do app para conversas sem projeto: o menu de pasta oferece "Trabalhar sem pasta".
  const [freeFolder, setFreeFolder] = useState<string | undefined>(undefined);
  useEffect(() => {
    freeWorkspace().then(setFreeFolder, () => setFreeFolder(undefined));
  }, []);
  useEffect(() => {
    let alive = true;
    Promise.resolve()
      .then(() => listGitBranches(cwd))
      .then(
        (list) => alive && setBranches(list),
        () => alive && setBranches([]),
      );
    currentGitBranch(cwd).then(
      (name) => alive && setCurrentBranch(name),
      () => alive && setCurrentBranch(null),
    );
    return () => {
      alive = false;
    };
  }, [cwd, branchesVersion]);
  useEffect(() => {
    onRunning?.(conversationId, running);
  }, [running]);
  const [notes, setNotes] = useState<string[]>([]);
  /// Volta no tempo esperando a confirmação do usuário.
  const [pendingRewind, setPendingRewind] = useState<{ index: number; checkpoint: string; text: string } | null>(null);
  const [host] = useState(() => new PluginHost());
  // Modos que os plugins oferecem; o escolhido vale só para o próximo envio.
  const [modes, setModes] = useState<PluginMode[]>([]);
  const [modeId, setModeId] = useState("");
  const responseText = useRef("");
  // Sessão nativa de cada agente e quanto do histórico ele já viu (ver chat/handoff.ts).
  const sessions = useRef<AgentSessions>(initialSessions(initial));
  // Agente do turno em andamento: os eventos da CLI não dizem de quem são.
  const turnProvider = useRef<ProviderKind>(provider);
  // Chave do agente do turno (ver chat/agents.ts) e nome do perfil, para marcar as respostas.
  const turnAgent = useRef<string>(agentKey(provider, profileId));
  const turnLabel = useRef<string | undefined>(undefined);
  // Aviso que vai à frente do próximo pedido depois de voltar a um checkpoint.
  const rewindNote = useRef<string | null>(null);
  // Autor das respostas salvas antes de cada mensagem guardar o agente.
  const legacyProvider = useRef<ProviderKind>(initial?.provider ?? provider);
  // null = ainda não verificado; false = precisa entrar na conta.
  const [loggedIn, setLoggedIn] = useState<boolean | null>(null);
  const [checking, setChecking] = useState(false);
  const [loginError, setLoginError] = useState<string | undefined>(undefined);
  // Painel da direita: aberto ou não, e qual aba.
  // Sempre começa fechado: o painel direito só abre quando o usuário pede.
  const [sideOpen, setSideOpen] = useState(false);
  const [sideTab, setSideTab] = useState<SideTab>("changes");
  // Modo navegador: a aba Navegador abre sozinha, e o chat lateral começa fechado.
  const [agentOpen, setAgentOpen] = useState(false);
  const [agentResizing, setAgentResizing] = useState(false);
  useEffect(() => {
    if (!browserMode) {
      // Voltar ao modo LunaCode fecha o painel direito, que era o navegador.
      setSideOpen(false);
      return;
    }
    setSideOpen(true);
    setSideTab("browser");
    setAgentOpen(false);
  }, [browserMode]);
  const [agentWidth, setAgentWidth] = useState(loadAgentWidth);
  const agentDrag = useRef<{ startX: number; startWidth: number } | null>(null);

  // A alça fica na borda esquerda do chat: arrastar para a esquerda alarga a sidebar.
  function onAgentResizeDown(e: ReactPointerEvent<HTMLDivElement>) {
    agentDrag.current = { startX: e.clientX, startWidth: agentWidth };
    setAgentResizing(true);
    e.currentTarget.setPointerCapture?.(e.pointerId);
  }

  function onAgentResizeMove(e: ReactPointerEvent<HTMLDivElement>) {
    if (!agentDrag.current) return;
    setAgentWidth(clampAgentWidth(agentDrag.current.startWidth + (agentDrag.current.startX - e.clientX), window.innerWidth));
  }

  function onAgentResizeUp(e: ReactPointerEvent<HTMLDivElement>) {
    if (!agentDrag.current) return;
    const final = clampAgentWidth(agentDrag.current.startWidth + (agentDrag.current.startX - e.clientX), window.innerWidth);
    agentDrag.current = null;
    setAgentResizing(false);
    setAgentWidth(final);
    saveAgentWidth(final);
  }

  function onAgentResizeKey(e: ReactKeyboardEvent<HTMLDivElement>) {
    const delta = e.key === "ArrowLeft" ? AGENT_KEY_STEP : e.key === "ArrowRight" ? -AGENT_KEY_STEP : 0;
    if (!delta) return;
    e.preventDefault();
    const next = clampAgentWidth(agentWidth + delta, window.innerWidth);
    setAgentWidth(next);
    saveAgentWidth(next);
  }
  // Arquivo aberto no editor, no lugar do chat. Fechar o editor volta ao chat.
  const [editingPath, setEditingPath] = useState<string | null>(null);
  // Outra pasta de projeto: o arquivo aberto era de outro lugar, então o editor fecha.
  useEffect(() => setEditingPath(null), [cwd]);
  // Link aberto com Ctrl+clique numa resposta: o painel do navegador abre uma aba nova com ele.
  const [browserRequest, setBrowserRequest] = useState<{ url: string } | null>(null);
  // Abre o terminal na primeira vez; depois ele só esconde, para não perder o shell.
  const [terminalOpened, setTerminalOpened] = useState(false);
  const [changesVersion, setChangesVersion] = useState(0);
  live.current = { provider, profileId, cwd, permission, model, effort, planning, running, sideTab };
  const [limits, setLimits] = useState<LimitState>({});
  const [context, setContext] = useState<ContextState | null>(null);
  // Tarefas do agente na resposta atual (aba Progresso), na ordem em que foram criadas.
  const [tasks, setTasks] = useState<TaskEntry[]>(() => initial?.tasks ?? []);
  // Ref para o listener, que é registrado uma vez: ele lê a lista atual daqui.
  const tasksRef = useRef<TaskEntry[]>(tasks);
  // Compactações da conversa: a última define onde começa a sessão nova (ver compact.ts).
  const [compactions, setCompactions] = useState<Compaction[]>(() => initial?.compactions ?? []);
  const compactionsRef = useRef<Compaction[]>(compactions);
  // compactPending: o próximo turno concluído é o resumo. compactDue: o contexto encheu, compactar depois do turno.
  const compactPending = useRef(false);
  const compactDue = useRef(false);
  // LunaFix: último comando do turno, correções pedidas neste envio, e a correção que espera a próxima rodada.
  const lastCommandRef = useRef<{ ok: boolean; output: string } | null>(null);
  const fixAttempts = useRef(0);
  const fixDue = useRef<string | null>(null);
  // Modo /meta: condição e rodadas já feitas do turno atual; a próxima rodada espera o fim do turno.
  const goalRef = useRef<{ condition: string; rounds: number } | null>(null);
  const goalDue = useRef<{ shown: string; prompt: string } | null>(null);
  const [goalOn, setGoalOn] = useState(false);
  // Meta em andamento: o botão fica ligado durante as rodadas e desmarcá-lo encerra o laço.
  const [goalActive, setGoalActive] = useState(false);
  const [speed, setSpeed] = useState<number | null>(null);
  const outputTokens = useRef(0);
  const firstTextAt = useRef<number | null>(null);
  // Velocidade medida: tokens de saída de cada chamada ao modelo, somados. O valor final fica no composer até o próximo turno.
  useEffect(() => {
    if (!running) return;
    outputTokens.current = 0;
    firstTextAt.current = null;
    setSpeed(null);
    const timer = window.setInterval(() => {
      setSpeed(tokensPerSecond(outputTokens.current, firstTextAt.current, Date.now()));
    }, 1000);
    return () => window.clearInterval(timer);
  }, [running]);
  // Parar no meio do turno: a verificação que ainda chegar não pede correção.
  const turnStopped = useRef(false);
  const body = useRef<HTMLDivElement>(null);
  const { stuck, scrollToBottom } = useStickToBottom(body, messages);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.ctrlKey && e.key === "`") {
        e.preventDefault();
        toggleTerminal();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
    // toggleTerminal só usa setters do React, que não mudam entre renderizações.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Função estável: o Markdown de cada mensagem só é refeito quando o link muda.
  // Função estável para a lista: a mensagem antiga não é refeita só porque a conversa mudou.
  const rewindRef = useRef(rewind);
  rewindRef.current = rewind;
  const onRewind = useCallback((m: ChatMessage) => {
    const index = messagesRef.current.indexOf(m);
    if (index >= 0 && m.checkpoint) void rewindRef.current(index, m.checkpoint);
  }, []);

  /// Abre o painel na aba pedida; se ela já está à mostra, fecha o painel.
  const openLinkInBrowser = useCallback((url: string) => {
    setBrowserRequest({ url });
    setSideOpen(true);
    setSideTab("browser");
  }, []);

  function toggleSide(tab: SideTab) {
    if (tab === "terminal") setTerminalOpened(true);
    setSideOpen((open) => !(open && live.current.sideTab === tab));
    setSideTab(tab);
  }

  function toggleTerminal() {
    toggleSide("terminal");
  }

  /// Desmarcar o /meta: com a meta rodando, encerra as rodadas; antes do envio, só desliga o modo.
  function toggleGoal() {
    if (!goalActive) {
      setGoalOn((on) => !on);
      return;
    }
    goalRef.current = null;
    goalDue.current = null;
    setGoalActive(false);
    setGoalOn(false);
  }

  /// Parar não depende do CLI: se ele travou, nenhum `done` chega. A tela destrava já; o backend encerra o processo.
  function stopRun() {
    turnStopped.current = true;
    fixDue.current = null;
    fixAttempts.current = AUTOFIX_MAX_ATTEMPTS;
    goalRef.current = null;
    goalDue.current = null;
    setGoalActive(false);
    setRunning(false);
    // Parar não passa pelo `done`: a lista de alterações precisa ser lida de novo mesmo assim.
    setChangesVersion((v) => v + 1);
    cancelPrompt(conversationId).catch((err) => {
      setNotes((current) => [...current, t("chat.stopFailed", { error: String(err) })]);
    });
  }

  // O StrictMode roda este efeito duas vezes em desenvolvimento: a carga dos plugins acontece uma vez só,
  // senão o mesmo modo seria registrado duas vezes no host.
  const pluginsLoad = useRef<Promise<void> | null>(null);
  useEffect(() => {
    if (!pluginsLoad.current) pluginsLoad.current = loadInstalledPlugins(host);
    void pluginsLoad.current.then(() => {
      setModes(host.modeList());
      setPluginThemes(host.themeList());
      setNotes(host.messages());
    }, () => showToast({ kind: "error", title: t("plugins.loadFailed") }));
    if (!cwd) {
      void getWorkspace().then(setCwd).catch((e) => setNotes([String(e)]));
    }
  }, [host]);

  useEffect(() => {
    void verifyLogin();
  }, [provider, profileId]);

  // Configurações instalou, removeu ou ligou/desligou um plugin: recarrega na hora, mesmo com resposta em andamento.
  // Um plugin desligado não pode ficar ativo até o fim da resposta: o reset já tira os hooks dele.
  useEffect(() => {
    const reload = () => {
      host.reset();
      void loadInstalledPlugins(host).then(() => {
        setModes(host.modeList());
        setPluginThemes(host.themeList());
        setNotes(host.messages());
      }, () => showToast({ kind: "error", title: t("plugins.loadFailed") }));
    };
    window.addEventListener(PLUGINS_CHANGED, reload);
    return () => window.removeEventListener(PLUGINS_CHANGED, reload);
  }, [host]);

  useEffect(() => {
    accountRef.current = account;
  }, [account]);

  // Contas extras: a página de Contas avisa quando cria uma; o chat relê a lista.
  useEffect(() => {
    const reload = () => {
      listAccounts()
        .then(setAccounts)
        .catch((e) => setNotes((current) => [...current, String(e)]));
    };
    reload();
    window.addEventListener(ACCOUNTS_CHANGED, reload);
    return () => window.removeEventListener(ACCOUNTS_CHANGED, reload);
  }, []);

  // Perfis e presets: recarregados quando a página de configurações muda a lista.
  useEffect(() => {
    const reload = () => {
      Promise.all([listApiProfiles(), apiPresets()])
        .then(([list, known]) => {
          setProfiles(list);
          setPresets(known);
        })
        .catch((e) => setNotes((current) => [...current, String(e)]));
    };
    reload();
    window.addEventListener(PROFILES_CHANGED, reload);
    return () => window.removeEventListener(PROFILES_CHANGED, reload);
  }, []);

  useEffect(() => {
    let active = true;
    // Perfil: os modelos que o usuário listou nele. Sem perfil: a lista que a CLI expõe.
    const profile = profileId ? profiles.find((p) => p.id === profileId) : undefined;
    const source: Promise<ModelOption[]> = profileId ? Promise.resolve(profileModelOptions(profile)) : listModels(provider);
    source
      .then((list) => {
        if (!active) return;
        setModels(list);
        // Sem escolha válida, usa o primeiro modelo da lista.
        const chosen = list.find((m) => m.id === live.current.model) ?? list.find((m) => m.default) ?? list[0];
        const supported = chosen?.efforts.map((e) => e.id) ?? [];
        setModel(chosen?.id ?? "");
        if (!supported.includes(live.current.effort)) setEffort("");
      })
      .catch((e) => {
        if (active) setNotes((current) => [...current, String(e)]);
      });
    return () => {
      active = false;
    };
  }, [provider, profileId, profiles]);

  useEffect(() => {
    /// Fim do turno, depois dos plugins. Com correção pedida, o aviso e o som esperam a correção terminar.
    const finishTurn = (fix: string | null, turn: { text: string; ok: boolean; lastCommand: { ok: boolean; output: string } | null }, subject: string) => {
      const goal = goalRef.current;
      let goalNote: string | null = null;
      if (goal && !turnStopped.current) {
        goal.rounds += 1;
        if (goalShouldContinue(turn.text, goal.rounds)) {
          goalDue.current = {
            shown: t("goal.shown", { round: goal.rounds + 1, max: MAX_GOAL_ROUNDS }),
            prompt: goalInstruction(goal.condition),
          };
          setGoalRound((n) => n + 1);
          return;
        }
        goalRef.current = null;
        setGoalActive(false);
        // O marcador é só para a Luna: sai do texto que o usuário vê.
        const last = [...messagesRef.current].reverse().find((m) => m.role === "assistant");
        if (last) commit(messagesRef.current.map((m) => (m === last ? { ...m, text: stripGoalMarker(m.text) } : m)));
        if (!isGoalDone(turn.text)) goalNote = t("goal.gaveUp", { max: MAX_GOAL_ROUNDS });
      }
      if (fix && !turnStopped.current && fixAttempts.current < AUTOFIX_MAX_ATTEMPTS) {
        fixAttempts.current += 1;
        fixDue.current = fix;
        setNotes((current) => [...current, t("autofix.fixing", { attempt: fixAttempts.current, max: AUTOFIX_MAX_ATTEMPTS })]);
        setFixRound((n) => n + 1);
        return;
      }
      // Sem correção possível, o aviso entra depois da lista de plugins: ela substitui as notas ao terminar.
      const gaveUp = fix && !turnStopped.current ? t("autofix.gaveUp", { error: (turn.lastCommand?.output ?? "").slice(-300) }) : null;
      void notifyIfUnfocused(
        live.current.planning ? t("chat.planReady") : t("chat.replyReady"),
        t("chat.finished", { subject }),
      );
      if (turn.ok) playReplySound();
      if (turn.ok) {
        showToast({
          kind: "success",
          title: live.current.planning ? t("chat.planReady") : t("chat.replyReady"),
          body: t("chat.finished", { subject }),
        });
      }
      setRunning(false);
      const extra = [gaveUp, goalNote].filter((note): note is string => !!note);
      void host.runAfterResponse(responseText.current).then(
        () => setNotes([...host.messages(), ...extra]),
        () => {
          showToast({ kind: "error", title: t("chat.hookFailed") });
          setNotes((current) => [...current, ...extra]);
        },
      );
    };
    const unlisten = onProviderEvent(conversationId, (event) => {
      const turn = turnAgent.current;
      if (event.kind === "textDelta" && firstTextAt.current === null) firstTextAt.current = Date.now();
      if (event.kind === "outputTokens") {
        outputTokens.current += event.tokens;
        setSpeed(tokensPerSecond(outputTokens.current, firstTextAt.current, Date.now()));
      }
      if (event.kind === "done") setSpeed(tokensPerSecond(outputTokens.current, firstTextAt.current, Date.now()));
      if (event.kind === "sessionStarted") {
        sessions.current = {
          ...sessions.current,
          [turn]: { id: event.sessionId, seen: sessions.current[turn]?.seen ?? 0 },
        };
      }
      if (event.kind === "textDelta") responseText.current += event.text;
      if (event.kind === "rateLimit") {
        setLimits((current) => ({
          ...current,
          [event.window]: { utilization: event.utilization, resetsAt: event.resetsAt },
        }));
      }
      if (event.kind === "context") {
        setContext({ tokens: event.tokens, windowSize: event.windowSize ?? undefined });
        // O próprio turno de resumo não dispara outro resumo.
        if (!compactPending.current && needsCompaction(event.tokens, event.windowSize ?? undefined) && autoCompactEnabled()) {
          compactDue.current = true;
        }
      }
      if (event.kind === "todos" || event.kind === "taskCreated" || event.kind === "taskAssigned" || event.kind === "taskUpdated") {
        tasksRef.current = applyTaskEvent(tasksRef.current, event);
        setTasks(tasksRef.current);
        // O progresso abre quando a resposta cria etapas. Num turno de planejamento o painel fica com o plano.
        if (tasksRef.current.length > 0 && !turnPlanning.current) {
          setSideOpen(true);
          setSideTab("progress");
        }
      }
      if (event.kind === "cost") {
        costRef.current += event.usd;
        setCostUsd(costRef.current);
      }
      // No plano, o texto do bloco luna-plano vai direto para o painel enquanto chega; o chat fica sem ele.
      const planStream = event.kind === "textDelta" && turnPlanning.current ? streamingPlan(responseText.current) : null;
      if (planStream !== null) {
        planRef.current = planStream;
        setPlan(planStream);
        setSideOpen(true);
        setSideTab("plan");
      } else {
        commitFrame(applyEvent(messagesRef.current, event, turnProvider.current, turnLabel.current));
      }
      // Fim da resposta e erro mostram o texto na hora, sem esperar o quadro seguinte.
      if (event.kind === "done" || event.kind === "error") frame.flush();
      if (event.kind === "cacheUsage") {
        turnUsage.current = { fresh: event.fresh, read: event.read, write: event.write };
      }
      if (event.kind === "commandResult") {
        lastCommandRef.current = { ok: event.ok, output: event.output };
        setBranchesVersion((v) => v + 1);
      }
      if (event.kind === "error") {
        void notifyIfUnfocused(t("chat.errorTitle"), t("chat.errorBody"));
        showToast({ kind: "error", title: t("chat.errorTitle"), body: t("chat.errorToastBody") });
      }
      if (event.kind === "done") {
        const usage = turnUsage.current;
        turnUsage.current = null;
        if (usage) recordTurn(usage, sessions.current[turn]?.id ?? null);
        // Plano concluído: a lista de progresso fecha. Um plano novo abre outra (ver approvePlan).
        if (tasksRef.current.length > 0 && tasksRef.current.every((t) => t.status === "completed")) {
          tasksRef.current = [];
          setTasks([]);
        }
        // O painel de progresso abre com a execução e fecha quando ela termina (outras abas ficam como estão).
        if (live.current.sideTab === "progress") setSideOpen(false);
        const firstUser = messagesRef.current.find((m) => m.role === "user");
        const subject = firstUser ? truncate(firstUser.text, 40) : t("chat.theConversation");
        const session = sessions.current[turn];
        // O agente viu tudo até o fim deste turno.
        if (session) sessions.current = { ...sessions.current, [turn]: { ...session, seen: messagesRef.current.length } };
        // Resumo pronto: a sessão antiga é descartada, e a próxima mensagem começa uma nova com o resumo.
        if (compactPending.current) {
          compactPending.current = false;
          if (event.ok && responseText.current.trim()) {
            compactionsRef.current = [...compactionsRef.current, { at: messagesRef.current.length, summary: responseText.current }];
            setCompactions(compactionsRef.current);
            sessions.current = {};
            setNotes((current) => [...current, t("chat.compacted")]);
          }
        }
        setChangesVersion((v) => v + 1);
        setBranchesVersion((v) => v + 1);
        checkConfig();
        // O bloco de perguntas sai do texto e vira o seletor; o resto da resposta fica no chat.
        const parsed: ReplyParts = turnAsk.current
          ? extractQuestions(responseText.current)
          : { text: responseText.current, questions: null };
        if (parsed.warning) setNotes((current) => [...current, parsed.warning!]);
        if (turnPlanning.current) setUnderstood(contextUnderstood(responseText.current));
        // Bloco escondido (válido ou malformado) sai do texto da mensagem, não só da exibição.
        if (parsed.text !== responseText.current) {
          const last = messagesRef.current.at(-1);
          if (last?.role === "assistant") commit([...messagesRef.current.slice(0, -1), { ...last, text: parsed.text }]);
        }
        if (parsed.questions) {
          // A única pergunta de um plano é a aprovação, que já tem o botão na barra: não repete no seletor.
          const approvalOnly = turnPlanning.current && parsed.questions.length === 1 && parsed.questions[0].options.includes(APPROVE_OPTION);
          if (!approvalOnly) {
            setQuestions(parsed.questions);
            setQuestionsRound((n) => n + 1);
          }
        }
        // Plugin criado pela Luna: o bloco vira um cartão para instalar, e o texto segue sem o bloco.
        const offer = extractPluginOffer(parsed.text);
        if (offer) {
          const last = messagesRef.current.at(-1);
          if (last?.role === "assistant") commit([...messagesRef.current.slice(0, -1), { ...last, text: offer.text }]);
          setPluginOffer(offer.folder);
        }
        // Vira plano a resposta sem perguntas, ou a que pede a aprovação (a aprovação é a última pergunta do plano).
        // Rodadas de perguntas não mexem no plano: o agente ainda está entendendo o contexto.
        // O plano completo vai para o painel; no chat fica só o resumo de 3 linhas.
        const planParts = turnPlanning.current ? extractPlan(parsed.text) : null;
        if (planParts) {
          const last = messagesRef.current.at(-1);
          if (last?.role === "assistant" && !last.tool) {
            commit([...messagesRef.current.slice(0, -1), { ...last, text: planParts.summary }]);
          } else if (planParts.summary) {
            // O plano chegou todo no painel: o resumo entra no chat como resposta nova.
            commit(applyEvent(messagesRef.current, { kind: "textDelta", text: planParts.summary }, turnProvider.current, turnLabel.current));
          }
        }
        if (turnPlanning.current) setPlanReady(planIsReady(parsed));
        const draft = planParts?.plan ?? planFromReply(parsed, turnPlanning.current);
        if (draft) {
          planRef.current = draft;
          setPlan(draft);
          setSideOpen(true);
          setSideTab("plan");
        }
        persist(messagesRef.current);
        const turnResult = {
          text: responseText.current,
          ok: event.ok,
          lastCommand: lastCommandRef.current,
          attempt: fixAttempts.current,
        };
        // Os plugins veem o fim do turno antes do aviso: uma correção segura a entrega até o agente terminar.
        void host.runTurnDone(turnResult).then(
          (fix) => finishTurn(fix, turnResult, subject),
          () => finishTurn(null, turnResult, subject),
        );
      }
    });
    const stopPermissions = onPermissionRequest(conversationId, (request) => {
      // Ferramenta liberada nesta conversa: responde sem mostrar o pedido de novo.
      if (alwaysAllowed.current.has(request.toolName)) {
        void answerPermission(request.id, true);
        return;
      }
      setPermissionRequests((current) => [...current, request]);
    });
    return () => {
      unlisten.then((stop) => stop());
      stopPermissions.then((stop) => stop());
    };
  }, [host, conversationId]);

  function decidePermission(request: PermissionRequest, allow: boolean, always = false) {
    if (allow && always) alwaysAllowed.current.add(request.toolName);
    setPermissionRequests((current) => current.filter((item) => item.id !== request.id));
    answerPermission(request.id, allow, allow ? undefined : t("permission.denied")).catch(() => {
      // Pedido já expirou ou foi respondido: nada a fazer além de tirá-lo da tela.
    });
  }

  function commit(next: ChatMessage[]) {
    messagesRef.current = next;
    setMessages(next);
  }

  /// Como commit, mas a tela só troca no próximo quadro: usado pelos pedaços da resposta.
  function commitFrame(next: ChatMessage[]) {
    messagesRef.current = next;
    frame.schedule();
  }

  function persist(current: ChatMessage[]) {
    if (current.length === 0) return;
    const firstUser = current.find((m) => m.role === "user");
    const title = firstUser ? truncate(firstUser.text, TITLE_MAX_CHARS) : t("sidebar.newConversation");
    // Lido da ref: o listener de eventos é registrado uma vez e não vê valores novos pelo closure.
    const latest = live.current;
    saveLastWorkspace(latest.cwd);
    onPersist({
      id: conversationId,
      title,
      provider: latest.provider,
      sessionId: sessions.current[agentKey(latest.provider, latest.profileId)]?.id,
      sessions: sessions.current,
      cwd: latest.cwd,
      permission: latest.permission,
      model: latest.model || undefined,
      effort: latest.effort || undefined,
      planning: latest.planning,
      profile: latest.profileId ?? undefined,
      costUsd: costRef.current || undefined,
      plan: planRef.current,
      specDir: specDirRef.current,
      account: accountRef.current || undefined,
      tasks: tasksRef.current,
      compactions: compactionsRef.current,
      createdAt: createdAt.current,
      updatedAt: Date.now(),
      messages: current,
    });
  }

  async function verifyLogin() {
    setChecking(true);
    try {
      setLoggedIn(profileId ? await apiProfileHasKey(profileId) : await checkAuth(provider));
      setLoginError(undefined);
    } catch (e) {
      setLoggedIn(false);
      setLoginError(String(e));
    } finally {
      setChecking(false);
    }
  }

  /// Trocar de pasta encerra a conversa atual: o id de sessão pertence à pasta anterior.
  function switchWorkspace(path: string) {
    if (path === cwd) return;
    setCwd(path);
    saveLastWorkspace(path);
    sessions.current = {};
    cacheTurns.current = [];
    host.setCacheSummary(null);
    commit([]);
  }

  async function browseWorkspace() {
    const picked = await pickWorkspace(cwd);
    if (picked) switchWorkspace(picked);
  }

  /// Trabalhar numa branch: cria a worktree dela (ou a branch nova, a partir da atual) e troca a conversa para a pasta nova.
  async function pickBranch(branch: string, isNew: boolean) {
    try {
      const path = await createGitWorktree(cwd, branch, isNew);
      switchWorkspace(path);
    } catch (e) {
      showToast({ kind: "error", title: t("chat.worktreeFailed"), body: String(e) });
    }
  }

  /// Envia um turno. `shown` aparece na conversa; `cliPrompt` é o que o CLI recebe.
  /// No modo plano, o CLI recebe a instrução de planejamento e roda só com leitura.
  /// Se o agente ainda não viu parte do histórico (troca de agente), o contexto vai antes da mensagem.
  /// Tira a impressão digital da configuração antes do turno. Falha só fica sem comparação, não trava o envio.
  function snapshotConfig() {
    configBefore.current = null;
    Promise.resolve()
      .then(() => configFingerprint())
      .then(
        (fingerprint) => {
          configBefore.current = fingerprint;
        },
        (e) => setNotes((current) => [...current, t("chat.configReadFailed", { error: String(e) })]),
      );
  }

  /// Depois do turno: se a configuração mudou, avisa as listas (MCP, skills, plugins) e oferece recarregar o app.
  function checkConfig() {
    const before = configBefore.current;
    configBefore.current = null;
    if (before === null) return;
    Promise.resolve()
      .then(() => configFingerprint())
      .then(
        (now) => {
          if (now === before) return;
          window.dispatchEvent(new Event(MCP_CHANGED));
          window.dispatchEvent(new Event(SKILLS_CHANGED));
          window.dispatchEvent(new Event(PLUGINS_CHANGED));
          setConfigChanged(true);
        },
        (e) => setNotes((current) => [...current, t("chat.configCheckFailed", { error: String(e) })]),
      );
  }

  async function run(shown: string, cliPrompt: string, plan: boolean, mode?: PluginMode, images: string[] = [], role: OrchestratorRole | null = null) {
    setRunning(true);
    setTurnStartedAt(Date.now());
    snapshotConfig();
    // O papel do orquestrador decide o motor e o modelo deste envio; sem escolha, vale a conversa atual.
    const orchestrator = await loadOrchestrator();
    // No modo navegador, o motor vem de Configurações > Navegador (sem escolha, vale a conversa atual).
    const browserAgent = browserMode ? getBrowserAgent() : undefined;
    const current = browserAgent
      ? { provider: browserAgent.provider, profileId: browserAgent.profileId ?? null, model: browserAgent.model, effort: browserAgent.effort ?? "" }
      : { provider, profileId, model: model ?? "", effort: effort ?? "" };
    const route = routeFor(role ?? (plan ? "plan" : mode?.id === "ask" ? "chat" : null), current, orchestrator);
    if (rewindNote.current) {
      cliPrompt = `${rewindNote.current}

${cliPrompt}`;
      rewindNote.current = null;
    }
    responseText.current = "";
    lastCommandRef.current = null;
    turnStopped.current = false;
    // Cada resposta monta o próprio checklist; o da resposta anterior não fica na aba.
    // As tarefas são da conversa inteira (o Claude Code as mantém na sessão): não zeram a cada envio.
    // O painel de progresso abre só quando o turno cria etapas (ver os eventos de tarefa).
    setQuestions(null);
    setUnderstood(null);
    turnPlanning.current = plan;
    // Perguntas: só o modo que declara `questions: false` fica sem seletor; o resto (incluindo o que não declara o campo) pergunta.
    turnAsk.current = turnAsksQuestions(plan, mode);
    turnUsage.current = null;
    turnMeta.current = { provider: route.provider, model: route.model, systemKey: [route.provider, route.profileId ?? "", route.model, route.effort].join("|") };
    turnProvider.current = route.provider;
    turnAgent.current = agentKey(route.provider, route.profileId);
    turnLabel.current = route.profileId ? options.find((o) => o.key === turnAgent.current)?.label : undefined;
    const history = messagesRef.current;
    // Os caminhos das imagens ficam na mensagem: o histórico mostra a miniatura, também depois de recarregar.
    const userMsg: ChatMessage = { role: "user", text: shown, at: Date.now(), ...(images.length > 0 ? { images } : {}) };
    commit([...history, userMsg]);
    // Quem envia quer ver a resposta: volta a seguir o fim.
    scrollToBottom();
    setNotes(host.messages());

    try {
      // Checkpoint antes de qualquer alteração do agente: é o ponto para o qual "Voltar até aqui" leva.
      const checkpoint = await checkpointTake(cwd).catch(() => null);
      if (checkpoint) commit(messagesRef.current.map((m) => (m === userMsg ? { ...m, checkpoint } : m)));
      const key = turnAgent.current;
      // Depois de uma compactação, só vale a sessão criada depois dela, e o histórico começa nela.
      const since = compactionsRef.current.at(-1);
      const cut = since?.at ?? 0;
      const visible = history.slice(cut);
      const sessionsSinceCut = sessionsFrom(sessions.current, cut);
      const behind = (sessionsSinceCut[key]?.seen ?? 0) < visible.length;
      const changes = behind ? await getChanges(cwd).catch(() => null) : null;
      const context = contextFor({
        sessions: sessionsSinceCut,
        target: key,
        history: visible,
        fallback: legacyProvider.current,
        cwd,
        changes,
      });
      const sessionId = context.sessionId;
      const preamble = since && !sessionId ? [compactPreamble(since.summary), context.preamble].filter(Boolean).join("\n\n") : context.preamble;
      await sendPrompt({
        provider: route.provider,
        profile: route.profileId ?? undefined,
        images,
        // Sem modo nem plano, o protocolo vem antes do contexto, para não cair entre o rótulo e a mensagem.
        prompt: (plan || mode ? "" : QUESTION_PROTOCOL + "\n\n") +
          withContext(preamble, plan ? withPlanInstruction(cliPrompt, loadPlanTarget()) : mode ? mode.instruction + "\n\n" + QUESTION_PROTOCOL + "\n\n" + cliPrompt : cliPrompt),
        sessionId,
        cwd,
        permission: plan || mode ? "readOnly" : permission,
        // Lido a cada envio: a escolha em Configurações vale para toda execução.
        browser: getAgentBrowser(),
        browserMode,
        model: route.model || undefined,
        effort: route.effort || undefined,
        contextWindow: route.provider === provider ? models.find((m) => m.id === route.model)?.contextWindow : undefined,
        subagents: !!orchestrator.subagent && route.provider === "claude",
        // O Planejar pesquisa as boas práticas antes do plano; fora dele e no modo navegador, a pesquisa fica fechada.
        webSearch: plan && !browserMode,
        longCache: longCacheEnabled(),
      }, conversationId, specDirRef.current, route.provider === provider && activeAccount ? activeAccount : undefined);
    } catch (e) {
      setRunning(false);
      setChangesVersion((v) => v + 1);
      commit(applyEvent(messagesRef.current, { kind: "error", message: String(e) }));
      persist(messagesRef.current);
    }
  }

  /// Anexa um arquivo: imagens viram miniatura; os demais ficam na lista, com o nome. Ambos vão para a pasta de anexos.
  async function attachFile(file: File) {
    if (file.size > 10 * 1024 * 1024) {
      setNotes([t("chat.fileTooBig", { name: file.name })]);
      return;
    }
    const image = ["image/png", "image/jpeg", "image/webp"].includes(file.type);
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = "";
      for (let i = 0; i < bytes.length; i += 0x8000) {
        binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      }
      const id = ++attachmentSeq.current;
      if (image) {
        const path = await saveAttachment(file.name, btoa(binary));
        setAttachments((current) => [...current, { id, path, url: URL.createObjectURL(file) }]);
      } else {
        const path = await saveFileAttachment(file.name, btoa(binary));
        setFileAttachments((current) => [...current, { id, name: file.name, path }]);
      }
    } catch (e) {
      setNotes([String(e)]);
    }
  }

  function removeAttachment(id: number) {
    const gone = attachments.find((a) => a.id === id);
    if (gone) URL.revokeObjectURL(gone.url);
    setAttachments((current) => current.filter((a) => a.id !== id));
  }

  function removeFileAttachment(id: number) {
    setFileAttachments((current) => current.filter((f) => f.id !== id));
  }

  // Menu de `/`: comandos da Luna, de plugin e as skills/comandos da CLI. As da CLI são lidas quando o menu abre.
  const [cliSlash, setCliSlash] = useState<SlashItem[]>([]);
  const slashOpen = slashText !== null;
  useEffect(() => {
    if (!slashOpen) return;
    let cancelled = false;
    listSlashItems(cwd, provider)
      .then((items) => {
        if (!cancelled) setCliSlash(items);
      })
      .catch(() => {
        if (!cancelled) setCliSlash([]);
      });
    return () => {
      cancelled = true;
    };
  }, [slashOpen, cwd, provider]);
  const pluginSlash: SlashItem[] = host.commandNames().map((name) => ({
    name,
    description: t("chat.pluginCommand"),
    origin: "plugin",
  }));
  const slashMatches = slashText === null ? [] : filterSlash([...LUNA_COMMANDS, ...pluginSlash, ...cliSlash], slashText);

  /// Escolher um item põe `/nome ` no campo, para o usuário completar o pedido.
  function pickSlash(item: SlashItem) {
    setDraft(`/${item.name} `);
  }

  /// Digitar "@" abre a lista de arquivos do projeto; o trecho depois do "@" filtra.
  function changeDraft(value: string) {
    draftRef.current = value;
    setSlashText(slashQuery(value));
    const match = /@([^s@]*)$/.exec(value);
    const request = ++fileRequest.current;
    if (!match) {
      // Sem "@", a lista já vazia fica como está: não re-renderiza a cada tecla.
      setFiles((current) => (current.length > 0 ? [] : current));
      return;
    }
    listFiles(cwd, match[1])
      .then((list) => {
        if (request === fileRequest.current) setFiles(list);
      })
      .catch(() => {
        if (request === fileRequest.current) setFiles([]);
      });
  }

  /// Escolhe um arquivo da lista: o "@" digitado vira o caminho completo.
  function pickFile(path: string) {
    setDraft(draftRef.current.replace(/@([^s@]*)$/, () => `@${path} `));
    picked.current.add(path);
    setFiles([]);
  }

  async function send() {
    const typed = draftRef.current.trim();
    const hasAnyAttachment = attachments.length > 0 || fileAttachments.length > 0;
    if ((!typed && !hasAnyAttachment) || running || loggedIn !== true) return;
    // Só com anexo, o pedido vai com uma frase padrão; os arquivos entram na lista abaixo.
    const prompt = typed || t("chat.seeAttachments");
    setDraft("");
    // Envio do usuário começa um ciclo novo de correções; a correção do LunaFix não passa por aqui.
    fixAttempts.current = 0;

    // `/plano` liga o modo de planejamento e não vai ao provedor.
    // `/plugin instalar <pasta>` abre o mesmo cartão de instalação que a Luna mostra depois de criar um plugin.
    const luna = parseLunaCommand(prompt);
    if (luna?.id === "plano") {
      setPlanning(!browserMode);
      return;
    }
    if (luna?.id === "compactar") {
      setDraft("");
      compact();
      return;
    }
    if (luna?.id === "plugin") {
      setPluginOffer(luna.folder);
      return;
    }
    // /projeto: só confere os arquivos que as CLIs leem; não cria nada.
    if (luna?.id === "projeto") {
      setDraft("");
      try {
        const files = await projectCheck(cwd);
        setNotes((current) => [...current, ...files.map((f) => t(f.found ? "project.found" : "project.missing", { name: f.name, purpose: f.purpose }))]);
      } catch (err) {
        setNotes((current) => [...current, t("project.error", { error: String(err) })]);
      }
      return;
    }
    // /subagente: tarefa própria do Claude, em segundo plano, enquanto a conversa segue livre.
    if (luna?.id === "subagente") {
      setDraft("");
      if (provider !== "claude") {
        setNotes((current) => [...current, t("chat.backgroundOnlyClaude")]);
        return;
      }
      const mode = modes.find((m) => m.id === modeId);
      try {
        const task = await startBackgroundTask(luna.task, cwd, planning || mode ? "readOnly" : permission, conversationId);
        setBackgroundTasks((current) => [...current.filter((t) => t.id !== task.id), task]);
      } catch (err) {
        setNotes((current) => [...current, t("chat.backgroundStartFailed", { error: String(err) })]);
      }
      return;
    }

    // Comando de plugin (`/nome args`): se devolver texto, esse texto é o que vai ao provedor.
    const command = await host.handleCommand(prompt);
    if (command.handled && command.prompt === undefined) {
      setNotes(host.messages());
      return;
    }
    // Skill ou comando da CLI (`/nome args`) vai como a CLI o entende; o que o usuário vê continua o mesmo.
    const outgoing = await host.runBeforeSend(command.handled ? command.prompt! : (cliPrompt(prompt, cliSlash) ?? prompt));
    // Arquivos citados com o "@" entram no pedido por nome, para o agente saber que devem ser lidos.
    const mentioned = [...picked.current].filter((p) => prompt.includes(`@${p}`));
    picked.current.clear();
    // Arquivos anexados (caminho na pasta de anexos) e citados com "@" entram juntos na lista de leitura.
    const readPaths = [...mentioned, ...fileAttachments.map((f) => f.path)];
    const withFiles =
      readPaths.length > 0
        ? `Arquivos citados no pedido (leia antes de responder): ${readPaths.join(", ")}

${outgoing}`
        : outgoing;
    // Modo de plugin: só leitura e com a instrução do modo na frente do pedido.
    // As imagens vão com este envio; a lista de anexos volta a vazia.
    const imagePaths = attachments.map((a) => a.path);
    const mode = modes.find((m) => m.id === modeId);
    attachments.forEach((a) => URL.revokeObjectURL(a.url));
    setAttachments([]);
    setFileAttachments([]);
    // /meta (comando ou ícone) vale só para este envio: a condição vira o pedido e o turno repete até o sinal.
    const goalCondition = luna?.id === "meta" ? luna.condition : goalOn ? prompt : null;
    setGoalOn(false);
    if (goalCondition) {
      goalRef.current = { condition: goalCondition, rounds: 0 };
      setGoalActive(true);
    }
    await run(prompt, goalCondition ? goalInstruction(goalCondition) : withFiles, planning, mode, imagePaths);
  }

  /// Guarda o uso de cache do turno e atualiza o resumo que o painel de cache lê (via host).
  function recordTurn(usage: CacheUsage, sessionId: string | null) {
    const meta = turnMeta.current;
    const now = Date.now();
    cacheTurns.current = [...cacheTurns.current, { usage, at: now, model: meta.model, systemKey: meta.systemKey, sessionId }];
    host.setCacheSummary(summarize(cacheTurns.current, { provider: meta.provider, longTtl: longCacheEnabled(), now }));
  }

  /// Volta os arquivos ao início deste turno e tira a conversa daquele ponto para baixo.
  /// O texto do pedido volta para o campo de envio, para o usuário poder reenviar ou editar.
  async function rewind(index: number, checkpoint: string) {
    if (running) return;
    let plan;
    try {
      plan = await checkpointPreview(cwd, checkpoint);
    } catch (e) {
      setNotes([String(e)]);
      return;
    }
    const text =
      plan.removed.length + plan.restored.length === 0
        ? t("chat.rewind.none")
        : t("chat.rewind.summary", { restored: plan.restored.length, removed: plan.removed.length });
    setPendingRewind({ index, checkpoint, text });
  }

  /// Depois da confirmação: restaura os arquivos e corta a conversa no ponto escolhido.
  async function doRewind({ index, checkpoint }: { index: number; checkpoint: string }) {
    try {
      await checkpointRestore(cwd, checkpoint);
    } catch (e) {
      setNotes([String(e)]);
      return;
    }
    const kept = messagesRef.current.slice(0, index);
    const text = messagesRef.current[index].text;
    commit(kept);
    // Sem sessão nativa, o próximo envio reenvia o histórico que sobrou, em texto.
    sessions.current = {};
    rewindNote.current = "Os arquivos foram restaurados para um ponto anterior desta conversa. Considere só o estado atual.";
    setDraft(text);
    persist(kept);
    setChangesVersion((n) => n + 1);
  }

  /// Respostas do seletor: viram a próxima mensagem do turno, no mesmo modo em que estava a conversa.
  async function answerQuestions(text: string) {
    if (running) return;
    // No modo plano, escolher a opção de aprovação encerra o planejamento e executa, como o botão faz.
    if (planning && text.split("\n").some((line) => line.endsWith(`: ${APPROVE_OPTION}`) || line === APPROVE_OPTION)) {
      await approvePlan();
      return;
    }
    // Respostas viram decisões da spec (decisions.md). Antes da spec existir, ficam guardadas até a aprovação.
    const lines = text.split("\n").map((line) => line.trim()).filter(Boolean);
    if (specDirRef.current && cwd) {
      void recordSpecDecisions(cwd, specDirRef.current, lines).catch((e) => console.error("não foi possível gravar as decisões da spec:", e));
    } else if (planning) {
      pendingRounds.current.push(lines);
    }
    await run(text, text, planning, modes.find((m) => m.id === modeId));
  }

  /// Aprovação do plano: sai do modo plano e manda o agente executar com a permissão escolhida.
  async function approvePlan() {
    if (running || loggedIn !== true) return;
    setPlanning(false);
    setPlanReady(false);
    // Execução nova: a lista de progresso começa do zero, sem as etapas de um plano anterior.
    tasksRef.current = [];
    setTasks([]);
    // Primeira aprovação: cria a pasta da spec e grava nela as rodadas de perguntas feitas antes.
    if (!specDirRef.current && cwd) {
      const firstUser = messagesRef.current.find((m) => m.role === "user");
      try {
        specDirRef.current = await createSpec(cwd, firstUser?.text ?? "spec");
        for (const lines of pendingRounds.current) await recordSpecDecisions(cwd, specDirRef.current, lines);
        pendingRounds.current = [];
      } catch (e) {
        console.error("não foi possível criar a pasta da spec:", e);
      }
    }
    // A instrução de registro das etapas depende do motor que vai executar (o do papel de execução, se houver).
    const engine = routeFor("execute", { provider, profileId, model: "", effort: "" }, await loadOrchestrator()).provider;
    await run(t("chat.planApproved"), approvalPrompt(planRef.current ?? null, engine, specDirRef.current), false, undefined, [], "execute");
  }

  function changeModel(next: string) {
    setModel(next);
    // Nível que o novo modelo não aceita volta para o padrão.
    const supported = models.find((m) => m.id === next)?.efforts.map((e) => e.id) ?? [];
    if (!supported.includes(effort)) setEffort("");
  }

  const agentK = agentKey(provider, profileId);
  // Contas em todas as CLIs, menos o Antigravity (ver accounts.rs), e não com perfil de API.
  const accountsEnabled = !profileId && provider !== "antigravity";
  const accountsOfEngine = accounts.filter((a) => a.provider === provider);
  const activeAccount = accountsOfEngine.some((a) => a.id === account) ? account : "";

  const options = agentOptions(profiles, presets);
  const agentLabel = options.find((o) => o.key === agentK)?.label ?? PROVIDER_LABELS[provider];

  /// Troca de agente: a conversa continua, e o novo agente recebe o histórico no próximo envio.
  function changeAgent(key: string) {
    const next = options.find((o) => o.key === key);
    if (!next) return;
    setProvider(next.engine);
    setProfileId(next.profileId ?? null);
    setModel("");
    setEffort("");
    // A conta pertence ao motor: trocar de motor volta para a conta padrão.
    setAccount("");
    // O Cline e o Hermes só rodam com acesso total: trocar para eles leva a permissão junto.
    if (next.engine === "cline" || next.engine === "hermes") setPermission("fullAccess");
  }

  // Sem conta conectada ou sem pasta de trabalho ainda carregada, não há como enviar.
  const blocked = loggedIn !== true || !cwd;
  const lastIsReply = messages.at(-1)?.role === "assistant";
  const modelWindow = models.find((m) => m.id === model)?.contextWindow;

  return (
    <div className={`chat${browserMode ? " chat--browser" : ""}`}>
      <div data-plugin-slot="chat" />
      {pendingRewind && (
        <ConfirmDialog
          title={t("chat.rewindTitle")}
          body={pendingRewind.text}
          confirmLabel={t("chat.rewindConfirm")}
          danger
          onCancel={() => setPendingRewind(null)}
          onConfirm={() => {
            const target = pendingRewind;
            setPendingRewind(null);
            void doRewind(target);
          }}
        />
      )}
      <header className="chat__header">
        <h2 className="chat__title">{messages.length > 0 ? titleOf(messages) : t("sidebar.newConversation")}</h2>
        <div className="chat__tools">
          {editingPath && (
            <button type="button" className="chat__tool" onClick={() => setEditingPath(null)}>
              <ArrowLeft size={14} aria-hidden="true" /> {t("chat.backToChat")}
            </button>
          )}
          <button
            type="button"
            className="chat__tool"
            aria-pressed={sideOpen && sideTab === "changes"}
            onClick={() => toggleSide("changes")}
          >
            <GitCompare size={14} aria-hidden="true" /> {t("chat.tool.changes")}
          </button>
          <button
            type="button"
            className="chat__tool"
            aria-pressed={sideOpen && sideTab === "plan"}
            onClick={() => toggleSide("plan")}
          >
            {t("chat.tool.plan")}
          </button>
          <button
            type="button"
            className="chat__tool"
            aria-pressed={sideOpen && sideTab === "terminal"}
            title={t("chat.tool.terminalTitle")}
            onClick={toggleTerminal}
          >
            <Terminal size={14} aria-hidden="true" /> {t("chat.tool.terminal")}
          </button>
          <button
            type="button"
            className="chat__tool"
            aria-pressed={sideOpen && sideTab === "browser"}
            title={t("chat.tool.browser")}
            onClick={() => toggleSide("browser")}
          >
            <Globe size={14} aria-hidden="true" /> {t("chat.tool.browser")}
          </button>
        </div>
      </header>

      <div className="chat__split">
      {editingPath && cwd && (
        <FileEditor key={`${cwd}:${editingPath}`} cwd={cwd} path={editingPath} onClose={() => setEditingPath(null)} />
      )}
      {/* Escondida, não desmontada: a resposta em andamento continua enquanto o editor está aberto.
          No modo navegador ela vira a sidebar do agente, com largura própria. */}
      <div
        className="chat__column"
        hidden={editingPath !== null}
        // No modo navegador a coluna abre e fecha pela largura; fechada, nada dela recebe foco.
        style={browserMode ? { width: agentOpen ? agentWidth : 0, flex: "0 0 auto" } : undefined}
        inert={browserMode && !agentOpen}
        data-resizing={agentResizing}
      >
      {browserMode && agentOpen && (
        <div
          className="chat__agent-resize"
          role="separator"
          aria-orientation="vertical"
          aria-label={t("chat.agentResize")}
          aria-valuenow={agentWidth}
          tabIndex={0}
          onPointerDown={onAgentResizeDown}
          onPointerMove={onAgentResizeMove}
          onPointerUp={onAgentResizeUp}
          onKeyDown={onAgentResizeKey}
        />
      )}
      <div className="chat__body" ref={body}>
        {loggedIn === false && profileId && (
          <p role="alert" className="chat__notes">
            {t("chat.profileNoKey")}
          </p>
        )}
        {loggedIn === false && !profileId && (
          <LoginPrompt
            provider={provider}
            onLogin={() => {
              openLogin(provider).catch((e) => setLoginError(String(e)));
            }}
            onCheck={() => void verifyLogin()}
            checking={checking}
            error={loginError}
          />
        )}

        {notes.length > 0 && (
          <ul className="chat__notes" aria-label={t("chat.notesAria")}>
            {notes.map((note, i) => (
              <li key={i}>{note}</li>
            ))}
          </ul>
        )}

        {messages.length === 0 ? (
          <div className="chat__empty">
            <img src="/luna/logo.svg" alt="" className="chat__logo" />
            <h1>{t("chat.emptyTitle")}</h1>
            <p>{planning ? t("chat.planningHint") : t("chat.talkingWith", { agent: agentLabel })}</p>
            {planning && (
              <button type="button" className="chat__skip" onClick={() => setPlanning(false)}>
                {t("chat.skipInterview")}
              </button>
            )}
            {!planning && (
              <div className="chat__suggestions">
                {SUGGESTIONS.map((key) => (
                  <button key={key} type="button" onClick={() => setDraft(t(key))}>
                    {t(key)}
                  </button>
                ))}
              </div>
            )}
          </div>
        ) : (
          <MessageList
            messages={messages}
            fallbackProvider={legacyProvider.current}
            running={running}
            pending={{ provider, label: agentLabel }}
            step={displayTodos(toTodos(tasks), running).find((todo) => todo.status === "in_progress")?.content ?? null}
            startedAt={turnStartedAt}
            onOpenLink={openLinkInBrowser}
            onRewind={onRewind}
          />
        )}

        {!stuck && messages.length > 0 && (
          <button type="button" className="chat__to-end" onClick={scrollToBottom}>
            {t("chat.toEnd")} <ArrowDown size={13} aria-hidden="true" />
          </button>
        )}

        {planning && planReady && lastIsReply && !running && (
          <div className="plan-approval" role="region" aria-label={t("chat.planApprovalAria")}>
            <p className="plan-approval__context">
              {understood === null
                ? t("chat.understoodUnknown")
                : t("chat.understood", { understood, target: loadPlanTarget() })}
            </p>
            <p>{t("chat.planApproveHint")}</p>
            <button type="button" onClick={() => void approvePlan()} disabled={blocked}>
              {t("chat.approveRun")}
            </button>
          </div>
        )}

      </div>

      {/* Acima da caixa de mensagem, como no Claude Desktop: fica fora da lista de mensagens. */}
      {questions && !running && (
        <QuestionPicker
          key={questionsRound}
          questions={questions}
          onAnswer={(text) => void answerQuestions(text)}
          onDismiss={() => setQuestions(null)}
        />
      )}

      {cloning && !running && (
        <SiteCloneFlow
          permission={permission}
          onPermission={setPermission}
          onSend={(shown, prompt, folder) => {
            setClonePreview(folder);
            void run(shown, prompt, false);
          }}
          onClose={() => setCloning(false)}
        />
      )}

      {clonePreview && cwd && !running && (
        <SiteClonePreview cwd={cwd} folder={clonePreview} onOpen={openLinkInBrowser} onClose={() => setClonePreview(null)} />
      )}

      {configChanged && (
        <section className="plugin-offer" role="status" aria-label={t("chat.configChangedAria")}>
          <p>
            <strong>{t("chat.configChangedText")}</strong> {t("chat.configChangedHint")}
          </p>
          <div className="api-form__actions">
            <button type="button" disabled={running} onClick={() => window.location.reload()}>
              {t("chat.reload")}
            </button>
            <button type="button" onClick={() => setConfigChanged(false)}>
              {t("chat.later")}
            </button>
          </div>
        </section>
      )}

      {pluginOffer && (
        <PluginOfferCard
          folder={pluginOffer}
          onInstalled={(message) => {
            setPluginOffer(undefined);
            setNotes((current) => [...current, message]);
          }}
          onDismiss={() => setPluginOffer(undefined)}
        />
      )}

      {backgroundTasks.length > 0 && <BackgroundTasks tasks={backgroundTasks} onStop={(id) => void stopBackground(id)} />}

      {permissionRequests[0] && (
        <PermissionPrompt
          request={permissionRequests[0]}
          onAllow={(request) => decidePermission(request, true)}
          onAlways={(request) => decidePermission(request, true, true)}
          onDeny={(request) => decidePermission(request, false)}
        />
      )}

      <CliUpdateNotice infos={updatesToShow(cliUpdates, running)} onUpdated={onCliUpdated} />

      <Composer
        composerRef={composerRef}
        browserMode={browserMode}
        onDraft={changeDraft}
        attachments={attachments}
        onAttach={(file) => void attachFile(file)}
        onRemoveAttachment={removeAttachment}
        fileAttachments={fileAttachments.map(({ id, name }) => ({ id, name }))}
        onRemoveFileAttachment={removeFileAttachment}
        files={files}
        onPickFile={pickFile}
        onSend={() => void send()}
        onStop={stopRun}
        running={running}
        speed={speed}
        blocked={blocked}
        agent={agentK}
        agents={options}
        onAgent={changeAgent}
        permission={permission}
        onPermission={setPermission}
        models={models}
        model={model}
        onModel={changeModel}
        effort={effort}
        onEffort={setEffort}
        modes={modes}
        mode={modeId}
        onMode={setModeId}
        planning={planning}
        onTogglePlanning={() => setPlanning((on) => !on)}
        goalOn={goalOn || goalActive}
        goalActive={goalActive}
        onToggleGoal={toggleGoal}
        onClone={() => setCloning(true)}
        cloneDisabledReason={cwd ? undefined : t("clone.noProject")}
        slash={slashMatches}
        onPickSlash={pickSlash}
        accountsEnabled={accountsEnabled}
        accounts={accountsOfEngine}
        account={activeAccount}
        onAccount={setAccount}
      />
      <div className="chat__foot">
        {/* No modo navegador a pasta e a branch ficam fora do rodapé: a sidebar é só para conversar. */}
        {!browserMode && (
          <WorkspaceChip
            cwd={cwd}
            recent={recentFolders}
            freeFolder={freeFolder}
            disabled={running}
            onPick={switchWorkspace}
            onBrowse={() => void browseWorkspace()}
            branches={branches}
            currentBranch={currentBranch}
            onOpenBranches={() => setBranchesVersion((v) => v + 1)}
            onPickBranch={(branch, isNew) => void pickBranch(branch, isNew)}
          />
        )}
        {provider === "claude" && (
          <UsageMeter
            // Limites de conta são da assinatura da CLI; um perfil de API não tem.
            limits={profileId ? {} : limits}
            // Com perfil de API, a janela vem do modelo escolhido; sem ela, o medidor mostra só os tokens.
            context={context && { tokens: context.tokens, windowSize: profileId ? modelWindow : context.windowSize }}
            windowHint={profileId && !modelWindow ? t("chat.windowHint") : undefined}
          />
        )}
        {browserMode && onNewSession && (
          <button type="button" className="btn btn--ghost btn--sm chat__new-session" onClick={onNewSession}>
            {t("chat.newSession")}
          </button>
        )}
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          title={t("chat.compactTitle")}
          disabled={running || messages.length === 0}
          onClick={compact}
        >
          {t("chat.compact")}
        </button>
      </div>
      </div>

      <RightPanel
        open={sideOpen}
        onClose={() => setSideOpen(false)}
        tab={sideTab}
        onTab={(tab) => {
          if (tab === "terminal") setTerminalOpened(true);
          setSideTab(tab);
        }}
        changes={cwd ? <ChangesPanel cwd={cwd} version={changesVersion} active={sideOpen && sideTab === "changes"} /> : null}
        files={cwd ? <FilesPanel cwd={cwd} activePath={editingPath} onOpen={setEditingPath} /> : null}
        plan={plan}
        todos={displayTodos(toTodos(tasks), running)}
        background={backgroundTasks.length > 0 ? <BackgroundTasks tasks={backgroundTasks} onStop={(id) => void stopBackground(id)} /> : null}
        running={running}
        terminal={terminalOpened && cwd ? <TerminalTabs key={cwd} cwd={cwd} /> : null}
        browser={
          <BrowserPanel
            active={visible && sideOpen && sideTab === "browser"}
            project={cwd || undefined}
            openRequest={browserRequest}
            toolbarStart={
              browserMode ? (
                <button type="button" className="side__luna" onClick={onExitBrowserMode}>
                  {t("sidebar.lunaCodeMode")}
                </button>
              ) : undefined
            }
            toolbarEnd={
              browserMode ? (
                <button
                  type="button"
                  className="side__agent-toggle"
                  aria-pressed={agentOpen}
                  aria-label={agentOpen ? t("chat.agentHide") : t("chat.agentShow")}
                  title={agentOpen ? t("chat.agentHide") : t("chat.agentShow")}
                  onClick={() => setAgentOpen((open) => !open)}
                >
                  {agentOpen ? <PanelRightClose size={15} aria-hidden="true" /> : <PanelRightOpen size={15} aria-hidden="true" />}
                </button>
              ) : undefined
            }
          />
        }
        browserMode={browserMode}
      />
      </div>
    </div>
  );
}

/// Sessões de uma conversa salva. Arquivos antigos só têm `sessionId`, que o agente viu até o fim.
function initialSessions(initial?: Conversation): AgentSessions {
  if (initial?.sessions) return initial.sessions;
  if (initial?.sessionId) {
    return { [agentKey(initial.provider, initial.profile)]: { id: initial.sessionId, seen: initial.messages.length } };
  }
  return {};
}

/// Título da conversa: a primeira mensagem do usuário, já truncada.
function titleOf(messages: ChatMessage[]): string {
  const first = messages.find((m) => m.role === "user");
  return first ? truncate(first.text, TITLE_MAX_CHARS) : t("sidebar.newConversation");
}

function truncate(text: string, max: number): string {
  const single = text.replace(/\s+/g, " ").trim();
  return single.length > max ? `${single.slice(0, max - 1)}…` : single;
}
