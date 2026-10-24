import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  browserSetSearchEngine,
  cliStatus,
  deleteConversation,
  listConversations,
  loadConversation,
  pickWorkspace,
  renameConversation,
  saveConversation,
} from "./api";
import { ChatView } from "./features/chat/ChatView";
import { SettingsView } from "./features/settings/SettingsView";
import { SetupScreen } from "./features/setup/SetupScreen";
import { needsCliSetup } from "./features/setup/needsCliSetup";
import { Onboarding, onboardingDone } from "./features/onboarding/Onboarding";
import { Sidebar } from "./features/sidebar/Sidebar";
import type { CliStatus, CliUpdateInfo, Conversation, ConversationSummary } from "./types";
import { t, useLocale } from "./i18n";
import { ConfirmDialog } from "./ui/ConfirmDialog";
import { Toaster } from "./ui/Toaster";
import { ModeWarp, type ModeWarpRun } from "./ui/ModeWarp";
import { loadLastWorkspace } from "./workspace";
import { startUpdateChecks } from "./updates";
import { startCliUpdateChecks } from "./chat/cliUpdates";
import { getSearchEngine } from "./chat/searchEngine";

/// Ação pendente de confirmação: o diálogo mostra o texto e roda `run` se o usuário confirmar.
interface Confirm {
  title: string;
  body?: ReactNode;
  label: string;
  danger?: boolean;
  run: () => Promise<void>;
}

/// Conversa aberta na barra. Cada uma fica montada: uma resposta em andamento segue
/// mesmo quando o usuário troca de conversa (ver `hidden` abaixo).
interface Chat {
  key: number;
  initial?: Conversation;
  /** Pasta de uma conversa nova. Ausente = pasta de onde o luna foi rodado. */
  startCwd?: string;
  /** Aberta no modo navegador: a conversa é salva como do navegador e some da lista do LunaCode. */
  browser?: boolean;
}

export function App() {
  const locale = useLocale();
  // Checagem de atualização ao abrir e a cada hora (ver `startUpdateChecks`).
  useEffect(() => startUpdateChecks(), []);
  // O agente do navegador usa o motor salvo; o app avisa a escolha ao abrir (ver BrowserSection ao trocar).
  useEffect(() => {
    void browserSetSearchEngine(getSearchEngine()).catch(() => {});
  }, []);
  // Versão nova das CLIs, checada no mesmo ritmo (ver `startCliUpdateChecks`).
  const [cliUpdates, setCliUpdates] = useState<CliUpdateInfo[]>([]);
  useEffect(() => startCliUpdateChecks(setCliUpdates), []);
  function onCliUpdated(info: CliUpdateInfo) {
    setCliUpdates((list) => list.map((item) => (item.provider === info.provider ? info : item)));
  }
  const [summaries, setSummaries] = useState<ConversationSummary[]>([]);
  const [listError, setListError] = useState<string | undefined>(undefined);
  // A primeira aba já abre na última pasta de projeto usada, se houver.
  const [chats, setChats] = useState<Chat[]>(() => [{ key: 0, startCwd: loadLastWorkspace() }]);
  const [activeKey, setActiveKey] = useState(0);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [runningIds, setRunningIds] = useState<string[]>([]);
  // Id salvo de cada aba, para reabrir uma conversa sem criar outra aba para ela.
  const chatIds = useRef<Record<number, string>>({});
  const nextKey = useRef(1);
  // O chat fica montado enquanto as configurações estão abertas, para não cortar uma resposta.
  const [view, setView] = useState<"chat" | "settings">("chat");
  // Modo navegador: a tela vira um navegador e o chat fica numa sidebar lateral.
  const [browserMode, setBrowserMode] = useState(false);
  // Efeito de entrada e saída do modo navegador; cada troca gera um id novo.
  const [warp, setWarp] = useState<ModeWarpRun | null>(null);
  function changeBrowserMode(on: boolean) {
    setBrowserMode(on);
    setWarp({ id: Date.now(), direction: on ? "in" : "out" });
  }

  // CLIs que faltam no computador: abre a tela de instalação por cima do app (só na abertura).
  const [missingCli, setMissingCli] = useState<CliStatus[] | null>(null);
  // Apresentação de primeira abertura: vem antes da tela de instalação das CLIs.
  const [showOnboarding, setShowOnboarding] = useState(() => !onboardingDone());
  const [confirming, setConfirming] = useState<Confirm | null>(null);

  // O idioma da página acompanha a escolha, para leitores de tela e corretor ortográfico.
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  useEffect(() => {
    void refreshList();
    cliStatus()
      .then((statuses) => {
        if (needsCliSetup(statuses)) setMissingCli(statuses);
      })
      .catch(() => {
        // Sem a checagem, o app segue: o erro de CLI não encontrada aparece na conversa.
      });
  }, []);

  async function refreshList() {
    try {
      setSummaries(await listConversations());
      setListError(undefined);
    } catch (e) {
      setListError(String(e));
    }
  }

  function addChat(chat: Omit<Chat, "key">) {
    const key = nextKey.current++;
    setChats((current) => [...current, { ...chat, key }]);
    setActiveKey(key);
    setActiveId(null);
    setView("chat");
  }

  /// Conversa nova no modo em que o app está, a menos que `browser` diga outra coisa.
  function newConversation(cwd?: string, browser = browserMode) {
    addChat({ startCwd: cwd ?? loadLastWorkspace(), browser });
  }

  /// Liga ou desliga o modo navegador. Conversas de um modo não aparecem no outro: com uma conversa do modo
  /// de origem aberta, abre uma conversa nova do modo escolhido em vez de mostrá-la.
  function setBrowser(on: boolean) {
    const active = chats.find((c) => c.key === activeKey);
    if (active && !!active.browser !== on) newConversation(undefined, on);
    setView("chat");
    changeBrowserMode(on);
  }

  /// Escolhe uma pasta e começa uma conversa nela. Cancelar o seletor não faz nada.
  async function addProject() {
    try {
      const picked = await pickWorkspace();
      if (picked) newConversation(picked);
    } catch (e) {
      setListError(String(e));
    }
  }

  async function openConversation(id: string) {
    const open = chats.find((c) => chatIds.current[c.key] === id);
    if (open) {
      setActiveKey(open.key);
      setActiveId(id);
      setView("chat");
      return;
    }
    try {
      const conversation = await loadConversation(id);
      chatIds.current[nextKey.current] = id;
      addChat({ initial: conversation, browser: conversation.browser ?? false });
      setActiveId(id);
    } catch (e) {
      setListError(String(e));
    }
  }

  async function removeConversation(id: string) {
    const title = summaries.find((s) => s.id === id)?.title ?? t("app.thisConversation");
    if (runningIds.includes(id)) {
      setListError(t("app.stillReplying", { title }));
      return;
    }
    setConfirming({
      title: t("app.deleteTitle", { title }),
      body: t("app.cannotUndo"),
      label: t("common.delete"),
      danger: true,
      run: async () => {
        try {
          await deleteConversation(id);
          const gone = chats.filter((c) => chatIds.current[c.key] === id);
          setChats((current) => current.filter((c) => !gone.includes(c)));
          if (activeId === id) newConversation();
          await refreshList();
        } catch (e) {
          setListError(String(e));
        }
      },
    });
  }

  async function renameSelected(id: string, title: string) {
    try {
      await renameConversation(id, title);
      await refreshList();
    } catch (e) {
      setListError(String(e));
    }
  }

  /// Remove o projeto da barra lateral apagando as conversas dele. Confirma antes, com a contagem.
  async function removeProject(cwd: string) {
    const name = cwd.split(/[\\/]/).filter(Boolean).pop() ?? cwd;
    const ids = summaries.filter((s) => s.cwd === cwd).map((s) => s.id);
    if (ids.some((id) => runningIds.includes(id))) {
      setListError(t("app.waitProjectReplies", { name }));
      return;
    }
    setConfirming({
      title: t("app.removeProjectTitle", { name }),
      body: t("app.removeProjectBody", { count: ids.length }),
      label: t("common.remove"),
      danger: true,
      run: async () => {
        try {
          for (const id of ids) await deleteConversation(id);
          const gone = chats.filter((c) => ids.includes(chatIds.current[c.key] ?? ""));
          setChats((current) => current.filter((c) => !gone.includes(c)));
          if (activeId && ids.includes(activeId)) newConversation();
          await refreshList();
        } catch (e) {
          setListError(String(e));
          await refreshList();
        }
      },
    });
  }

  function onPersist(key: number, conversation: Conversation) {
    chatIds.current[key] = conversation.id;
    if (key === activeKey) setActiveId(conversation.id);
    // A origem vem da aba, não do que o chat envia: o chat não sabe em que modo a conversa nasceu.
    const browser = chats.find((c) => c.key === key)?.browser ?? false;
    saveConversation({ ...conversation, browser })
      .then(refreshList)
      .catch((e) => setListError(String(e)));
  }

  function onRunning(id: string, running: boolean) {
    setRunningIds((current) => {
      const without = current.filter((x) => x !== id);
      return running ? [...without, id] : without;
    });
  }

  // Pastas de conversas anteriores, sem repetir: alimentam o menu de troca no rodapé do chat.
  const recentFolders = [...new Set(summaries.map((s) => s.cwd))];

  return (
    <div className={`app${browserMode ? " app--browser" : ""}`}>
      <Toaster />
      {showOnboarding ? (
        <Onboarding onDone={() => setShowOnboarding(false)} />
      ) : (
        missingCli && <SetupScreen statuses={missingCli} onSkip={() => setMissingCli(null)} />
      )}
      {confirming && (
        <ConfirmDialog
          title={confirming.title}
          body={confirming.body}
          confirmLabel={confirming.label}
          danger={confirming.danger}
          onCancel={() => setConfirming(null)}
          onConfirm={() => {
            const run = confirming.run;
            setConfirming(null);
            void run();
          }}
        />
      )}
      <Sidebar
        conversations={summaries}
        activeId={activeId}
        runningIds={runningIds}
        onNew={(cwd) => newConversation(cwd)}
        onAddProject={() => void addProject()}
        onOpen={(id) => void openConversation(id)}
        onDelete={(id) => void removeConversation(id)}
        onRename={(id, title) => void renameSelected(id, title)}
        onRemoveProject={(cwd) => void removeProject(cwd)}
        onSettings={() => setView((v) => (v === "settings" ? "chat" : "settings"))}
        settingsOpen={view === "settings"}
        browserMode={browserMode}
        onBrowserMode={setBrowser}
        error={listError}
      />
      <ModeWarp run={warp} />
      <main className="app__main">
      {view === "settings" && <SettingsView onClose={() => setView("chat")} />}
      {chats.map((chat) => (
        <div key={chat.key} className="app__view" hidden={view !== "chat" || chat.key !== activeKey}>
          <ChatView
            initial={chat.initial}
            startCwd={chat.startCwd}
            recentFolders={recentFolders}
            onPersist={(conversation) => onPersist(chat.key, conversation)}
            onRunning={onRunning}
            visible={view === "chat" && chat.key === activeKey}
            browserMode={browserMode}
            onExitBrowserMode={() => setBrowser(false)}
            onNewSession={() => newConversation()}
            cliUpdates={cliUpdates}
            onCliUpdated={onCliUpdated}
          />
        </div>
      ))}
      </main>
    </div>
  );
}
