import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BackgroundTask, CliUpdateInfo, Conversation, ProviderEvent } from "../../types";
import { PluginHost } from "../../plugins/host";
import { ChatView } from "./ChatView";

// Substitui a ponte com o Rust: o teste controla quando os eventos do CLI chegam.
const bridge = vi.hoisted(() => ({
  emit: null as null | ((event: ProviderEvent) => void),
  background: null as null | ((task: unknown) => void),
}));

vi.mock("../../api", () => ({
  checkAuth: vi.fn(async () => true),
  currentGitBranch: vi.fn(async () => null),
  listGitBranches: vi.fn(async () => [] as string[]),
  createGitWorktree: vi.fn(async () => "C:\\projeto-feat-nova"),
  freeWorkspace: vi.fn(async () => "C:\app\Sem pasta"),
  onBrowserNavigated: vi.fn(async () => () => {}),
  openLogin: vi.fn(async () => {}),
  cancelPrompt: vi.fn(async () => {}),
  checkpointTake: vi.fn(async () => null),
  checkpointPreview: vi.fn(async () => ({ removed: [], restored: [] })),
  checkpointRestore: vi.fn(async () => ({ removed: [], restored: [] })),
  sendPrompt: vi.fn(async () => {}),
  listAccounts: vi.fn(async () => []),
  ACCOUNTS_CHANGED: "luna:accounts-changed",
  getOrchestrator: vi.fn(async () => ({})),
  getWorkspace: vi.fn(async () => "C:\\projeto"),
  listModels: vi.fn(async () => []),
  apiPresets: vi.fn(async () => []),
  apiProfileHasKey: vi.fn(async () => true),
  listApiProfiles: vi.fn(async () => []),
  listFiles: vi.fn(async () => []),
  saveAttachment: vi.fn(async () => "C:\\anexos\\foto.png"),
  readAttachment: vi.fn(async () => ({ mime: "image/png", data: "iVBORw0KGgo=" })),
  saveFileAttachment: vi.fn(async () => "C:\\anexos\\notas.md"),
  PROFILES_CHANGED: "luna:profiles-changed",
  getChanges: vi.fn(async () => ({ isRepo: false, files: [] })),
  getFileDiff: vi.fn(async () => ""),
  terminalOpen: vi.fn(async () => "term-1"),
  terminalWrite: vi.fn(async () => {}),
  terminalResize: vi.fn(async () => {}),
  terminalClose: vi.fn(async () => {}),
  browserNewTab: vi.fn(async () => 1),
  browserSwitchTab: vi.fn(async () => {}),
  browserCloseTab: vi.fn(async () => null),
  browserState: vi.fn(async () => ({ tabs: [], active: null })),
  browserBookmarks: vi.fn(async () => []),
  browserSetProject: vi.fn(async () => {}),
  browserBookmarkAdd: vi.fn(async () => []),
  browserBookmarkRemove: vi.fn(async () => []),
  browserSetBounds: vi.fn(async () => {}),
  browserShow: vi.fn(async () => {}),
  browserNavigate: vi.fn(async (url: string) => url),
  browserHistory: vi.fn(async () => {}),
  browserReload: vi.fn(async () => {}),
  onBrowserOpened: vi.fn(async () => () => {}),
  onTerminalData: vi.fn(async () => () => {}),
  onTerminalExit: vi.fn(async () => () => {}),
  pickWorkspace: vi.fn(async () => null),
  listPlugins: vi.fn(async () => ({ plugins: [], errors: [] })),
  readPluginEntry: vi.fn(async () => ""),
  getPluginsState: vi.fn(async () => ({ disabled: [] })),
  PLUGINS_CHANGED: "luna:plugins-changed",
  MCP_CHANGED: "luna:mcp-changed",
  SKILLS_CHANGED: "luna:skills-changed",
  configFingerprint: vi.fn(async () => "igual"),
  onProviderEvent: vi.fn(async (_runId: string, handler: (event: ProviderEvent) => void) => {
    bridge.emit = handler;
    return () => {};
  }),
  onPermissionRequest: vi.fn(async () => () => {}),
  onBrowserBookmarksChanged: vi.fn(async () => () => {}),
  onBrowserShortcut: vi.fn(async () => () => {}),
  browserLoginSuggestions: vi.fn(async () => [] as string[]),
  browserFillLogin: vi.fn(async () => {}),
  browserFavicon: vi.fn(async () => null),
  answerPermission: vi.fn(async () => {}),
  startBackgroundTask: vi.fn(async (prompt: string) => ({ id: "t1", prompt, status: "running", output: "", conversationId: "conv-teste" })),
  stopBackgroundTask: vi.fn(async (id: string) => ({ id, prompt: "", status: "stopped", output: "" })),
  listBackgroundTasks: vi.fn(async () => []),
  onBackgroundTask: vi.fn(async (handler: (task: unknown) => void) => {
    bridge.background = handler;
    return () => {};
  }),
}));

import * as api from "../../api";

function emit(event: ProviderEvent) {
  act(() => bridge.emit!(event));
}

async function send(text: string) {
  fireEvent.change(screen.getByLabelText("Mensagem"), { target: { value: text } });
  fireEvent.click(screen.getByLabelText("Enviar"));
  await waitFor(() => expect(screen.getByLabelText("Parar")).toBeTruthy());
}

/// Abre o seletor de provedor e escolhe o agente pelo nome que ele mostra.
async function pickAgent(name: string) {
  fireEvent.click(screen.getByRole("button", { name: "Provedor" }));
  fireEvent.click(await screen.findByRole("option", { name }));
}

describe("ChatView: continuidade e envio", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("sem escolha salva, começa pelo modelo marcado como padrão (o mais barato)", async () => {
    vi.mocked(api.listModels).mockResolvedValueOnce([
      { id: "claude-opus-5-5", label: "Opus 5.5", description: null, efforts: [], default: false },
      { id: "claude-haiku-5-5", label: "Haiku 5.5", description: null, efforts: [], default: true },
    ]);
    render(<ChatView onPersist={() => {}} />);

    expect(await screen.findByText("Haiku 5.5")).toBeTruthy();
  });

  it("conversa nova começa com o modo plano desligado, e só liga com o clique", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    expect(screen.getByLabelText("Modo de planejamento").getAttribute("aria-pressed")).toBe("false");
    expect(screen.queryByText(/Modo de planejamento/)).toBeNull();

    fireEvent.click(screen.getByLabelText("Modo de planejamento"));

    expect(screen.getByLabelText("Modo de planejamento").getAttribute("aria-pressed")).toBe("true");
    expect(await screen.findByText(/Modo de planejamento/)).toBeTruthy();
  });

  it("no modo plano, a resposta vai para a aba Plano do painel e abre a aba", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    fireEvent.click(screen.getByLabelText("Modo de planejamento"));

    await send("quero um app de tarefas");
    emit({ kind: "textDelta", text: "## Plano\n\nPasso 1: criar a tela." });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    expect(vi.mocked(api.sendPrompt).mock.calls[0][0].permission).toBe("readOnly");
    expect(screen.getByRole("tab", { name: "Plano" }).getAttribute("aria-selected")).toBe("true");
    expect(document.querySelector(".side__plan")?.textContent).toContain("Passo 1: criar a tela.");
  });

  it("no modo plano, uma resposta só com perguntas não vira plano", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    fireEvent.click(screen.getByLabelText("Modo de planejamento"));

    await send("quero um app de tarefas");
    emit({
      kind: "textDelta",
      text: 'Antes de planejar:\n```luna-perguntas\n[{"pergunta": "Qual banco?", "opcoes": ["Postgres", "SQLite"]}]\n```',
    });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    expect(screen.getByRole("tab", { name: "Plano" }).getAttribute("aria-selected")).toBe("false");
    expect(document.querySelector(".side__plan")?.textContent ?? "").not.toContain("Antes de planejar");
  });

  it("compactar pede o resumo e a próxima mensagem começa com ele, sem a sessão antiga", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    await send("primeira pergunta");
    emit({ kind: "textDelta", text: "Resposta antiga." });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByRole("button", { name: "Compactar" }).hasAttribute("disabled")).toBe(false));

    fireEvent.click(screen.getByRole("button", { name: "Compactar" }));
    await waitFor(() => expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(2));
    emit({ kind: "textDelta", text: "## Objetivo\nTela de login." });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByText(/Conversa compactada/)).toBeTruthy());

    await send("continua");
    await waitFor(() => expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(3));
    const next = vi.mocked(api.sendPrompt).mock.calls[2][0];
    expect(next.prompt).toContain("Esta conversa foi compactada");
    expect(next.prompt).toContain("Tela de login.");
    expect(next.sessionId).toBeUndefined();
  });

  it("no plano, o chat mostra só o resumo e o painel Plano mostra o plano completo", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    fireEvent.click(screen.getByLabelText("Modo de planejamento"));

    await send("quero um app de tarefas");
    emit({
      kind: "textDelta",
      text: "Resumo: criar a tela de tarefas.\n```luna-plano\n## Arquivos a alterar\n- src/App.tsx: monta a tela\n```\nPode aprovar?",
    });
    emit({ kind: "done", ok: true });

    await waitFor(() => expect(document.querySelector(".side__plan")?.textContent ?? "").toContain("src/App.tsx"));
    expect(screen.getByText(/Resumo: criar a tela/).textContent).not.toContain("src/App.tsx");
  });

  it("no modo plano, uma rodada de perguntas não mostra o botão de aprovação", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    fireEvent.click(screen.getByLabelText("Modo de planejamento"));

    await send("quero um app de tarefas");
    emit({
      kind: "textDelta",
      text: 'Contexto entendido: 30%\n```luna-perguntas\n[{"pergunta": "Qual banco?", "opcoes": ["Postgres", "SQLite"]}]\n```',
    });
    emit({ kind: "done", ok: true });

    await screen.findByRole("region", { name: "Perguntas do agente" });
    expect(screen.queryByRole("region", { name: "Aprovação do plano" })).toBeNull();
  });

  it("no modo plano, o botão de aprovação aparece quando a resposta traz o plano pronto", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    fireEvent.click(screen.getByLabelText("Modo de planejamento"));

    await send("quero um app de tarefas");
    emit({
      kind: "textDelta",
      text: "Resumo: criar a tela de tarefas.\n```luna-plano\n## Arquivos a alterar\n- src/App.tsx: monta a tela\n```",
    });
    emit({ kind: "done", ok: true });

    expect(await screen.findByRole("region", { name: "Aprovação do plano" })).toBeTruthy();
  });

  it("no modo plano, responder 'Aprovar e executar' pelo seletor desliga o plano e executa", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    fireEvent.click(screen.getByLabelText("Modo de planejamento"));

    await send("quero um app de tarefas");
    emit({
      kind: "textDelta",
      text: 'Posso começar?\n```luna-perguntas\n[{"pergunta": "Aprovar o plano?", "opcoes": ["Aprovar e executar", "Ainda não"]}]\n```',
    });
    emit({ kind: "done", ok: true });
    // A pergunta de aprovação não vira seletor: o botão da barra de aprovação é o único controle.
    const bar = within(await screen.findByRole("region", { name: "Aprovação do plano" }));
    expect(screen.queryByRole("region", { name: "Perguntas do agente" })).toBeNull();
    fireEvent.click(bar.getByRole("button", { name: "Aprovar e executar" }));

    await waitFor(() => expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(2));
    const approval = vi.mocked(api.sendPrompt).mock.calls[1][0];
    expect(approval.prompt).toContain("execute o plano agora");
    expect(approval.permission).not.toBe("readOnly");
    expect(screen.getByLabelText("Modo de planejamento").getAttribute("aria-pressed")).toBe("false");
  });

  it("envio comum ensina o bloco de perguntas e mostra as perguntas no seletor", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    await send("crie uma tela de login");
    expect(vi.mocked(api.sendPrompt).mock.calls[0][0].prompt).toContain("luna-perguntas");
    emit({
      kind: "textDelta",
      text: 'Antes de começar:\n```luna-perguntas\n[{"pergunta": "Qual banco?", "opcoes": ["Postgres", "SQLite"]}]\n```',
    });
    emit({ kind: "done", ok: true });

    const picker = within(await screen.findByRole("region", { name: "Perguntas do agente" }));
    expect(picker.getByRole("button", { name: "Postgres" })).toBeTruthy();
    expect(screen.queryByText(/luna-perguntas/)).toBeNull();
  });

  it("bloco de perguntas sem a cerca de fechamento não aparece no chat e o aviso aparece", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    await send("crie uma tela de login");
    emit({
      kind: "textDelta",
      text: 'Antes de começar:\n```luna-perguntas\n[{"pergunta": "Qual banco?", "opcoes": ["Postgres", "SQLite"]}]',
    });
    emit({ kind: "done", ok: true });

    expect(await screen.findByText(/formato inválido/)).toBeTruthy();
    expect(screen.queryByText(/luna-perguntas/)).toBeNull();
    expect(screen.queryByRole("region", { name: "Perguntas do agente" })).toBeNull();
  });

  it("sugestões da tela vazia preenchem a caixa sem enviar", async () => {
    render(<ChatView onPersist={() => {}} />);

    fireEvent.click(await screen.findByRole("button", { name: "Encontre bugs" }));

    expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).value).toBe("Encontre bugs");
    expect(api.sendPrompt).not.toHaveBeenCalled();
  });

  it("perfil de API vira o agente: manda o id e o motor, e a resposta leva o nome do perfil", async () => {
    vi.mocked(api.apiPresets).mockResolvedValueOnce([
      { preset: "deepSeek", label: "DeepSeek", engine: "claude", defaultBaseUrl: "https://api.deepseek.com/anthropic", suggestedModels: [], defaultModel: "" },
    ]);
    vi.mocked(api.listApiProfiles).mockResolvedValueOnce([
      { id: "p1", name: "DeepSeek", preset: "deepSeek", baseUrl: "", models: ["deepseek-chat"], defaultModel: "deepseek-chat" },
    ]);
    render(<ChatView onPersist={() => {}} />);

    await screen.findByRole("button", { name: "Provedor" });
    await pickAgent("DeepSeek");
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("olá");
    emit({ kind: "textDelta", text: "oi" });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    const request = vi.mocked(api.sendPrompt).mock.calls[0][0];
    expect(request.profile).toBe("p1");
    expect(request.provider).toBe("claude");
    expect(document.querySelector(".message__author")?.textContent).toContain("DeepSeek");
  });

  it("anexar um arquivo que não é imagem e enviar sem texto manda o caminho salvo no pedido", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    const input = document.querySelector<HTMLInputElement>('input[type="file"]');
    fireEvent.change(input!, { target: { files: [new File(["# notas"], "notas.md", { type: "text/markdown" })] } });
    await screen.findByText("notas.md");

    fireEvent.click(screen.getByLabelText("Enviar"));
    await waitFor(() => expect(api.sendPrompt).toHaveBeenCalledTimes(1));

    expect(api.saveFileAttachment).toHaveBeenCalledWith("notas.md", expect.any(String));
    const request = vi.mocked(api.sendPrompt).mock.calls[0][0];
    expect(request.prompt).toContain("C:\\anexos\\notas.md");
    expect(request.prompt).toContain("Veja os anexos.");
    expect(request.images ?? []).toHaveLength(0);
  });

  it("envia o sessionId da primeira resposta na segunda mensagem", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("primeira");
    emit({ kind: "sessionStarted", sessionId: "sess-1" });
    emit({ kind: "textDelta", text: "resposta" });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    await send("segunda");

    const first = vi.mocked(api.sendPrompt).mock.calls[0][0];
    const second = vi.mocked(api.sendPrompt).mock.calls[1][0];
    expect(first.sessionId).toBeUndefined();
    expect(second.sessionId).toBe("sess-1");
    expect(second.cwd).toBe("C:\\projeto");
    expect(second.permission).toBe("fullAccess");
  });

  it("mostra a velocidade medida pelos tokens de saída, sem botão, e a mantém depois do fim do turno", async () => {
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    try {
      render(<ChatView onPersist={() => {}} />);
      await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

      await send("velocidade");
      now.mockReturnValue(2_000);
      emit({ kind: "textDelta", text: "resposta" });
      now.mockReturnValue(3_000);
      emit({ kind: "outputTokens", tokens: 50 });
      expect(screen.getByText("50 tokens/s")).toBeTruthy();

      now.mockReturnValue(4_000);
      emit({ kind: "done", ok: true });
      // 50 tokens desde o primeiro texto (2 s até 4 s) = 25 tokens/s, com o turno já encerrado.
      await waitFor(() => expect(screen.getByText("25 tokens/s")).toBeTruthy());
    } finally {
      now.mockRestore();
    }
  });

  it("sem tokens medidos, a resposta não mostra velocidade nenhuma, nem zero", async () => {
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    try {
      render(<ChatView onPersist={() => {}} />);
      await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

      await send("sem medição");
      now.mockReturnValue(2_000);
      emit({ kind: "textDelta", text: "resposta" });
      now.mockReturnValue(3_000);

      expect(screen.queryByText(/tokens\/s/)).toBeNull();
      expect(screen.queryByLabelText("Mostrar velocidade da resposta")).toBeNull();
    } finally {
      now.mockRestore();
    }
  });

  it("cada envio leva o navegador escolhido em Configurações", async () => {
    localStorage.setItem("luna.agentBrowser", "playwright");
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("usa o playwright");

    expect(vi.mocked(api.sendPrompt).mock.calls[0][0].browser).toBe("playwright");
    localStorage.removeItem("luna.agentBrowser");
  });

  it("troca de agente no meio da conversa sem perder contexto", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("crie a landing page");
    emit({ kind: "sessionStarted", sessionId: "sess-claude" });
    emit({ kind: "textDelta", text: "Criei index.html." });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    // Claude → Codex: sessão nova com o histórico inteiro antes da mensagem.
    await pickAgent("Codex");
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    await send("agora ajuste o CSS");
    const toCodex = vi.mocked(api.sendPrompt).mock.calls[1][0];
    expect(toCodex.provider).toBe("codex");
    expect(toCodex.sessionId).toBeUndefined();
    expect(toCodex.prompt).toContain("crie a landing page");
    expect(toCodex.prompt).toContain("### Claude Code\nCriei index.html.");
    expect(toCodex.prompt).toMatch(/Nova mensagem do usuário:\nagora ajuste o CSS$/);
    expect(screen.getByText("Criei index.html.")).toBeTruthy();

    emit({ kind: "sessionStarted", sessionId: "sess-codex" });
    emit({ kind: "textDelta", text: "Ajustei o CSS." });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    // Codex → Claude: retoma a sessão antiga e recebe só o que perdeu.
    await pickAgent("Claude Code");
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    await send("revise tudo");
    const back = vi.mocked(api.sendPrompt).mock.calls[2][0];
    expect(back.sessionId).toBe("sess-claude");
    expect(back.prompt).toContain("### Codex\nAjustei o CSS.");
    expect(back.prompt).not.toContain("crie a landing page");
  });

  it("salva a conversa completa ao fim do turno, com o sessionId", async () => {
    const persist = vi.fn();
    render(<ChatView onPersist={persist} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("pergunta");
    emit({ kind: "sessionStarted", sessionId: "sess-9" });
    emit({ kind: "textDelta", text: "resposta final" });
    emit({ kind: "done", ok: true });

    await waitFor(() => expect(persist).toHaveBeenCalled());
    const saved = persist.mock.calls.at(-1)![0];
    expect(saved.sessionId).toBe("sess-9");
    expect(saved.title).toBe("pergunta");
    expect(saved.cwd).toBe("C:\\projeto");
    expect(saved.messages).toEqual([
      { role: "user", text: "pergunta", at: expect.any(Number) },
      { role: "assistant", text: "resposta final", provider: "claude", at: expect.any(Number) },
    ]);
    expect(saved.sessions).toEqual({ claude: { id: "sess-9", seen: 2 } });
  });
});

describe("Progresso de um plano novo", () => {
  it("aprovar um plano novo começa a lista de progresso do zero, sem as etapas do plano anterior", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    // Um plano anterior ainda não terminou: a lista de progresso fica cheia depois da resposta.
    await send("primeiro pedido");
    emit({ kind: "todos", items: [{ content: "Etapa antiga", status: "in_progress" }] });
    emit({ kind: "done", ok: true });
    // O fim do turno passa pelos plugins antes de liberar o envio.
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());
    fireEvent.click(screen.getByRole("tab", { name: "Progresso" }));
    expect(screen.getByText("Etapa antiga")).toBeTruthy();

    // Outro plano: planejar, aprovar e executar.
    fireEvent.click(screen.getByLabelText("Modo de planejamento"));
    await send("outra coisa");
    emit({
      kind: "textDelta",
      text: 'Posso começar?\n```luna-perguntas\n[{"pergunta": "Aprovar o plano?", "opcoes": ["Aprovar e executar", "Ainda não"]}]\n```',
    });
    emit({ kind: "done", ok: true });
    const bar = within(await screen.findByRole("region", { name: "Aprovação do plano" }));
    fireEvent.click(bar.getByRole("button", { name: "Aprovar e executar" }));
    await waitFor(() => expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(3));

    expect(screen.queryByText("Etapa antiga")).toBeNull();
  });

  it("quando todas as etapas terminam, a lista de progresso fecha", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("plano curto");
    emit({
      kind: "todos",
      items: [
        { content: "Ler o arquivo", status: "completed" },
        { content: "Ajustar o texto", status: "completed" },
      ],
    });
    emit({ kind: "done", ok: true });
    fireEvent.click(screen.getByRole("tab", { name: "Progresso" }));

    expect(screen.queryByText("Ler o arquivo")).toBeNull();
    expect(screen.getByText(/Quando o agente montar as etapas/)).toBeTruthy();
  });

  it("envio sem etapas não abre o painel de progresso", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    const panel = screen.getByRole("complementary", { name: "Painel lateral" });

    await send("pergunta simples");
    expect(panel.style.width).toBe("0px");
    emit({ kind: "textDelta", text: "Resposta direta." });
    emit({ kind: "done", ok: true });
    expect(panel.style.width).toBe("0px");
  });

  it("o painel de progresso abre quando a tarefa cria etapas e fecha quando a execução termina", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    const panel = screen.getByRole("complementary", { name: "Painel lateral" });

    await send("tarefa com etapas");
    expect(panel.style.width).toBe("0px");
    emit({ kind: "todos", items: [{ content: "Ler o arquivo", status: "in_progress" }] });
    expect(panel.style.width).not.toBe("0px");

    emit({ kind: "done", ok: true });
    expect(panel.style.width).toBe("0px");
  });
});

describe("ChatView: recarregar depois de instalar MCP, skill ou plugin", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("quando a configuração muda durante o turno, oferece recarregar e avisa as listas", async () => {
    vi.mocked(api.configFingerprint).mockResolvedValueOnce("antes").mockResolvedValueOnce("depois");
    const onMcp = vi.fn();
    window.addEventListener("luna:mcp-changed", onMcp);
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("instale o mcp de docs");
    emit({ kind: "textDelta", text: "Instalado." });
    emit({ kind: "done", ok: true });

    expect(await screen.findByRole("button", { name: "Recarregar" })).toBeTruthy();
    expect(onMcp).toHaveBeenCalledOnce();
    window.removeEventListener("luna:mcp-changed", onMcp);
  });

  it("sem mudança na configuração, não aparece aviso de recarregar", async () => {
    vi.mocked(api.configFingerprint).mockResolvedValueOnce("igual").mockResolvedValueOnce("igual");
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("oi");
    emit({ kind: "textDelta", text: "Oi!" });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    expect(screen.queryByRole("button", { name: "Recarregar" })).toBeNull();
  });

  it("Parar destrava a tela mesmo quando o CLI não manda o fim da resposta", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("oi");
    // O CLI travou na inicialização: nenhum evento `done` chega.
    fireEvent.click(screen.getByLabelText("Parar"));

    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());
    expect(api.cancelPrompt).toHaveBeenCalled();
  });
});

describe("ChatView: navegador só mostra a página na conversa visível", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
  });

  it("conversa oculta não mostra a página do navegador ao abrir a aba", async () => {
    render(<ChatView onPersist={() => {}} visible={false} />);
    fireEvent.click(screen.getByTitle("Navegador"));
    await act(async () => {});
    expect(api.browserShow).not.toHaveBeenCalledWith(true);
  });
});

describe("ChatView: LunaFix (correção depois de comando que falhou)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  /// Em cada fim de turno, o plugin pede (ou não) uma correção. O teste troca só essa resposta.
  function plugin(answers: (string | null)[]) {
    return vi.spyOn(PluginHost.prototype, "runTurnDone").mockImplementation(async () => answers.shift() ?? null);
  }

  it("comando que falhou e plugin pedindo correção: manda a correção na mesma conversa", async () => {
    plugin(["corrija o teste", null]);
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("implemente a soma");
    emit({ kind: "commandResult", ok: false, output: "Exit code 1" });
    emit({ kind: "textDelta", text: "Pronto." });
    emit({ kind: "done", ok: true });

    await waitFor(() => expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.sendPrompt).mock.calls[1][0].prompt).toContain("corrija o teste");
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());
  });

  it("depois de duas correções, entrega com aviso em vez de tentar de novo", async () => {
    plugin(["corrija 1", "corrija 2", "corrija 3"]);
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("implemente a soma");
    for (let turn = 0; turn < 3; turn++) {
      emit({ kind: "commandResult", ok: false, output: "falha persistente" });
      emit({ kind: "done", ok: true });
      await waitFor(() => expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(Math.min(turn + 2, 3)));
    }

    expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(3);
    expect(await screen.findByText(/não conseguiu corrigir o erro do último comando/)).toBeTruthy();
  });
});

describe("Orquestrador: papel escolhido troca o motor do envio", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("o papel de planejar manda o envio para o motor escolhido em Configurações", async () => {
    vi.mocked(api.getOrchestrator).mockResolvedValueOnce({ plan: { provider: "codex", model: "gpt-5.5" } });
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    fireEvent.click(screen.getByLabelText("Modo de planejamento"));
    await send("planeje isto");

    expect(vi.mocked(api.sendPrompt).mock.calls.at(-1)?.[0]).toMatchObject({ provider: "codex", model: "gpt-5.5" });
  });

  it("sem escolha para o papel, o envio segue o motor da conversa", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("oi");

    expect(vi.mocked(api.sendPrompt).mock.calls.at(-1)?.[0]).toMatchObject({ provider: "claude" });
  });
});

describe("ChatView: branch da pasta", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.listGitBranches).mockResolvedValue([]);
    vi.mocked(api.currentGitBranch).mockResolvedValue(null);
  });

  it("branch criada pelo agente aparece no menu e vira a atual, sem trocar de pasta", async () => {
    vi.mocked(api.listGitBranches).mockResolvedValue(["main"]);
    vi.mocked(api.currentGitBranch).mockResolvedValue("main");
    render(<ChatView onPersist={() => {}} />);
    await screen.findByRole("button", { name: "main" });

    await send("crie a branch feat/nova e trabalhe nela");
    vi.mocked(api.listGitBranches).mockResolvedValue(["feat/nova", "main"]);
    vi.mocked(api.currentGitBranch).mockResolvedValue("feat/nova");
    emit({ kind: "commandResult", ok: true, output: "Switched to a new branch 'feat/nova'" });
    emit({ kind: "done", ok: true });

    fireEvent.click(await screen.findByRole("button", { name: "feat/nova" }));
    expect(await screen.findByRole("menuitem", { name: "main" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: /feat\/nova/ })).toBeTruthy();
  });
});

describe("ChatView: imagem no histórico", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    URL.createObjectURL = vi.fn(() => "blob:foto");
    URL.revokeObjectURL = vi.fn();
    vi.mocked(api.readAttachment).mockResolvedValue({ mime: "image/png", data: "iVBORw0KGgo=" });
  });

  it("imagem enviada continua no balão do usuário depois da resposta", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    const input = document.querySelector<HTMLInputElement>('input[type="file"]');
    fireEvent.change(input!, { target: { files: [new File(["png"], "foto.png", { type: "image/png" })] } });
    await screen.findByRole("img", { name: "Imagem anexada" });

    await send("veja isto");
    emit({ kind: "textDelta", text: "Vi a foto." });
    emit({ kind: "done", ok: true });

    expect(await screen.findByRole("img", { name: "Imagem anexada" })).toBeTruthy();
    expect(vi.mocked(api.sendPrompt).mock.calls[0][0].images).toEqual(["C:\\anexos\\foto.png"]);
  });

  it("a mensagem salva com imagem mostra a miniatura ao reabrir a conversa", async () => {
    const saved: Conversation = {
      id: "c-foto",
      title: "Foto",
      provider: "claude",
      cwd: "C:\\projeto",
      permission: "acceptEdits",
      planning: false,
      createdAt: 1,
      updatedAt: 2,
      messages: [{ role: "user", text: "veja isto", at: 1, images: ["C:\\anexos\\foto.png"] }],
    };
    render(<ChatView initial={saved} onPersist={() => {}} />);

    expect(await screen.findByRole("img", { name: "Imagem anexada" })).toBeTruthy();
    expect(vi.mocked(api.readAttachment)).toHaveBeenCalledWith("C:\\anexos\\foto.png");
  });

  it("imagem que não existe mais mostra um aviso curto, sem quebrar a conversa", async () => {
    vi.mocked(api.readAttachment).mockRejectedValueOnce(new Error("arquivo não encontrado"));
    const saved: Conversation = {
      id: "c-sumiu",
      title: "Foto",
      provider: "claude",
      cwd: "C:\\projeto",
      permission: "acceptEdits",
      planning: false,
      createdAt: 1,
      updatedAt: 2,
      messages: [{ role: "user", text: "veja isto", at: 1, images: ["C:\\anexos\\sumiu.png"] }],
    };
    render(<ChatView initial={saved} onPersist={() => {}} />);

    expect((await screen.findAllByText("veja isto")).length).toBeGreaterThan(0);
    expect(await screen.findByText("Imagem indisponível")).toBeTruthy();
  });
});

function emitBackground(task: BackgroundTask) {
  act(() => bridge.background!(task));
}

describe("ChatView: subagente em segundo plano", () => {
  const conversation: Conversation = {
    id: "conv-teste",
    title: "Teste",
    provider: "claude",
    cwd: "C:\\projeto",
    permission: "acceptEdits",
    planning: false,
    createdAt: 1,
    updatedAt: 2,
    messages: [],
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("/subagente inicia a tarefa e a conclusão vira mensagem na conversa", async () => {
    render(<ChatView initial={conversation} onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    fireEvent.change(screen.getByLabelText("Mensagem"), { target: { value: "/subagente revisar os testes" } });
    fireEvent.click(screen.getByLabelText("Enviar"));

    await waitFor(() =>
      expect(vi.mocked(api.startBackgroundTask)).toHaveBeenCalledWith("revisar os testes", "C:\\projeto", "acceptEdits", "conv-teste"),
    );
    expect(vi.mocked(api.sendPrompt)).not.toHaveBeenCalled();

    emitBackground({ id: "t1", prompt: "revisar os testes", status: "done", output: "tudo certo", conversationId: "conv-teste" });

    expect(await screen.findByText(/tudo certo/)).toBeTruthy();
    expect(vi.mocked(api.sendPrompt)).not.toHaveBeenCalled();
  });

  it("a conclusão não interrompe um turno em andamento", async () => {
    render(<ChatView initial={conversation} onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("oi");
    emitBackground({ id: "t2", prompt: "outra tarefa", status: "done", output: "resultado pronto", conversationId: "conv-teste" });

    expect(await screen.findByText(/resultado pronto/)).toBeTruthy();
    expect(screen.getByLabelText("Parar")).toBeTruthy();
    expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(1);
  });

  it("parar uma tarefa atualiza o status na lista", async () => {
    vi.mocked(api.listBackgroundTasks).mockResolvedValueOnce([
      { id: "t3", prompt: "tarefa longa", status: "running", output: "", conversationId: "conv-teste" },
    ]);
    render(<ChatView initial={conversation} onPersist={() => {}} />);

    fireEvent.click(await screen.findByRole("button", { name: "Parar tarefa" }));

    await waitFor(() => expect(vi.mocked(api.stopBackgroundTask)).toHaveBeenCalledWith("t3"));
    expect((await screen.findAllByText("Parada")).length).toBeGreaterThan(0);
  });

  it("resultado de tarefa de outra conversa não entra nesta", async () => {
    render(<ChatView initial={conversation} onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    emitBackground({
      id: "t9",
      prompt: "tarefa alheia",
      status: "done",
      output: "não deve aparecer",
      conversationId: "conv-outra",
    });

    expect(screen.queryByText(/não deve aparecer/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Parar tarefa" })).toBeNull();
  });
});

describe("ChatView: modo navegador", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("com o modo ligado, o navegador ocupa a tela e o chat fica como coluna lateral montada", async () => {
    render(<ChatView onPersist={() => {}} browserMode />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    expect(document.querySelector(".chat")!.className).toContain("chat--browser");
    expect(screen.getByRole("complementary", { name: "Painel lateral" }).className).toContain("side--expanded");
    // O chat continua montado: a resposta em andamento não morre.
    expect(screen.getByLabelText("Mensagem")).toBeTruthy();
  });

  it("no modo navegador, o composer esconde permissão e as ações que começam edições (Planejar e /meta)", async () => {
    render(<ChatView onPersist={() => {}} browserMode />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    expect(screen.queryByRole("button", { name: "Permissão" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Modo de planejamento" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Modo /meta" })).toBeNull();
  });

  it("fora do modo navegador, o composer mantém permissão, Planejar e /meta", async () => {
    render(<ChatView onPersist={() => {}} browserMode={false} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    expect(screen.getByRole("button", { name: "Permissão" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Modo de planejamento" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Modo /meta" })).toBeTruthy();
  });

  it("sem o modo, o painel não fica expandido e o chat não vira coluna lateral", async () => {
    render(<ChatView onPersist={() => {}} browserMode={false} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    expect(document.querySelector(".chat")!.className).not.toContain("chat--browser");
    expect(screen.getByRole("complementary", { name: "Painel lateral" }).className).not.toContain("side--expanded");
  });
});

describe("ChatView: chat lateral no modo navegador", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("começa fechado e o botão da topbar abre com o chat à vista", async () => {
    render(<ChatView onPersist={() => {}} browserMode />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    const column = document.querySelector(".chat__column")!;
    expect(column.hasAttribute("inert")).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Mostrar o chat" }));

    expect(column.hasAttribute("inert")).toBe(false);
    expect(screen.getByRole("button", { name: "Ocultar o chat" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("o Modo LunaCode da topbar avisa a tela para voltar ao normal", async () => {
    const onExitBrowserMode = vi.fn();
    render(<ChatView onPersist={() => {}} browserMode onExitBrowserMode={onExitBrowserMode} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    fireEvent.click(screen.getByRole("button", { name: "Modo LunaCode" }));

    expect(onExitBrowserMode).toHaveBeenCalledOnce();
  });

  it("no modo navegador, Nova sessão abre outra sessão; sem o modo, o botão não aparece", async () => {
    const onNewSession = vi.fn();
    const { rerender } = render(<ChatView onPersist={() => {}} browserMode onNewSession={onNewSession} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    fireEvent.click(screen.getByRole("button", { name: "Nova sessão" }));
    expect(onNewSession).toHaveBeenCalledOnce();

    rerender(<ChatView onPersist={() => {}} browserMode={false} onNewSession={onNewSession} />);
    expect(screen.queryByRole("button", { name: "Nova sessão" })).toBeNull();
  });

  it("no modo navegador a pasta e a branch saem do rodapé", async () => {
    render(<ChatView onPersist={() => {}} browserMode />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    expect(document.querySelector(".chat__foot .workspace-chip, .chat__foot .workspace")).toBeNull();
    expect(screen.getByRole("button", { name: "Compactar" })).toBeTruthy();
  });
});

describe("ChatView: sair do modo navegador fecha o painel lateral", () => {
  it("ao voltar ao modo normal, o painel direito fecha", async () => {
    const { rerender } = render(<ChatView onPersist={() => {}} browserMode />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    expect(screen.getByRole("complementary", { name: "Painel lateral" }).className).not.toContain("side--closed");

    rerender(<ChatView onPersist={() => {}} browserMode={false} />);

    expect(screen.getByRole("complementary", { name: "Painel lateral" }).className).toContain("side--closed");
  });
});

describe("ChatView: /meta repete o pedido até o agente sinalizar o fim", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("/meta repete sem o sinal e para quando o marcador chega, sem mostrá-lo", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("/meta todos os testes passam");
    emit({ kind: "textDelta", text: "ainda falta um teste" });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.sendPrompt).mock.calls[0][0].prompt).toContain("todos os testes passam");

    emit({ kind: "textDelta", text: "feito\nMETA_CONCLUIDA" });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());
    expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(2);
    expect(screen.getByText("feito")).toBeTruthy();
    expect(screen.queryByText(/META_CONCLUIDA/)).toBeNull();
  });

  it("sem sinal, para no teto de 20 rodadas e avisa", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("/meta organizar o módulo");
    for (let round = 1; round <= 20; round++) {
      emit({ kind: "textDelta", text: "ainda não" });
      emit({ kind: "done", ok: true });
      await waitFor(() => expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(Math.min(round + 1, 20)));
    }

    expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(20);
    expect(await screen.findByText(/não foi confirmada em 20 rodadas/)).toBeTruthy();
  });

  it("o ícone vale só para a mensagem em que foi ligado", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    fireEvent.click(screen.getByLabelText("Modo /meta"));
    await send("organizar o módulo");
    expect(vi.mocked(api.sendPrompt).mock.calls[0][0].prompt).toContain("organizar o módulo");
    emit({ kind: "textDelta", text: "feito\nMETA_CONCLUIDA" });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    await send("oi");
    expect(vi.mocked(api.sendPrompt).mock.calls[1][0].prompt).not.toContain("META_CONCLUIDA");
    expect(screen.getByLabelText("Modo /meta").getAttribute("aria-pressed")).toBe("false");
  });
});

describe("ChatView: Parar encerra o /meta sem nova rodada", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("parar no meio do /meta não envia a próxima rodada", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("/meta organizar o módulo");
    fireEvent.click(screen.getByLabelText("Parar"));
    emit({ kind: "textDelta", text: "ainda não" });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(1);
  });
});

describe("ChatView: desmarcar a meta durante a resposta encerra o /meta", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("com a meta ativa, o botão segue ligado e habilitado; desmarcar impede a próxima rodada", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("/meta organizar o módulo");
    const goal = screen.getByRole("button", { name: "Modo /meta" }) as HTMLButtonElement;
    expect(goal.disabled).toBe(false);
    expect(goal.getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(goal);
    expect(screen.getByRole("button", { name: "Modo /meta" }).getAttribute("aria-pressed")).toBe("false");
    emit({ kind: "textDelta", text: "ainda não" });
    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());

    expect(vi.mocked(api.sendPrompt)).toHaveBeenCalledTimes(1);
  });
});

describe("ChatView: cabeçalho sem custo e sem lista de comandos", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("o cabeçalho não mostra o custo da conversa nem a lista de comandos", async () => {
    const conversation: Conversation = {
      id: "conv-custo",
      title: "Custo",
      provider: "claude",
      cwd: "C:\projeto",
      permission: "acceptEdits",
      planning: false,
      createdAt: 1,
      updatedAt: 2,
      messages: [],
      costUsd: 209.12,
    };
    render(<ChatView initial={conversation} onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    expect(screen.queryByText(/nesta conversa/)).toBeNull();
    expect(screen.queryByText(/Comandos:/)).toBeNull();
  });
});

describe("ChatView: clonador no chat", () => {
  it("o botão Clonar site abre o seletor de destino e partes, e o fluxo segue para o formulário", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    fireEvent.click(screen.getByRole("button", { name: "Clonar um site para o projeto (beta)" }));
    fireEvent.click(await screen.findByRole("button", { name: "Criar uma subpasta com o nome escolhido" }));
    fireEvent.click(await screen.findByRole("button", { name: "Cores" }));
    fireEvent.click(screen.getByRole("button", { name: "Enviar respostas" }));

    expect(await screen.findByLabelText("Nome da pasta do clone")).toBeTruthy();
  });
});

describe("Modo navegador: o motor do chat vem de Configurações do navegador", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it("no modo navegador, o envio usa o motor e o modelo escolhidos nas configurações do navegador", async () => {
    localStorage.setItem("luna.browserAgent", JSON.stringify({ provider: "codex", model: "gpt-5.5" }));
    render(<ChatView onPersist={() => {}} browserMode />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("resuma esta página");

    expect(vi.mocked(api.sendPrompt).mock.calls.at(-1)?.[0]).toMatchObject({ provider: "codex", model: "gpt-5.5" });
  });

  it("no modo LunaCode, o envio ignora a escolha do navegador e segue a conversa", async () => {
    localStorage.setItem("luna.browserAgent", JSON.stringify({ provider: "codex", model: "gpt-5.5" }));
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("oi");

    expect(vi.mocked(api.sendPrompt).mock.calls.at(-1)?.[0]).toMatchObject({ provider: "claude" });
  });
});

describe("Modo navegador: instruções próprias e esforço do envio", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it("no modo navegador, o envio pede as instruções do navegador", async () => {
    render(<ChatView onPersist={() => {}} browserMode />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("o que você faz?");

    expect(vi.mocked(api.sendPrompt).mock.calls.at(-1)?.[0]).toMatchObject({ browserMode: true });
  });

  it("no LunaCode, o envio não pede as instruções do navegador", async () => {
    render(<ChatView onPersist={() => {}} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("oi");

    expect(vi.mocked(api.sendPrompt).mock.calls.at(-1)?.[0]).toMatchObject({ browserMode: false });
  });

  it("no modo navegador, o envio usa o esforço escolhido nas configurações", async () => {
    localStorage.setItem("luna.browserAgent", JSON.stringify({ provider: "codex", model: "gpt-5.5", effort: "high" }));
    render(<ChatView onPersist={() => {}} browserMode />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));

    await send("resuma esta página");

    expect(vi.mocked(api.sendPrompt).mock.calls.at(-1)?.[0]).toMatchObject({ provider: "codex", model: "gpt-5.5", effort: "high" });
  });
});

describe("ChatView: aviso de versão nova da CLI", () => {
  const update: CliUpdateInfo = {
    provider: "claude",
    installed: "1.0.0",
    latest: "9.9.9",
    updateAvailable: true,
    canUpdate: true,
    error: null,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("mostra o aviso com o agente parado e esconde enquanto ele responde", async () => {
    render(<ChatView onPersist={() => {}} cliUpdates={[update]} />);
    await waitFor(() => expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(false));
    expect(screen.getByText(/9\.9\.9 disponível/)).toBeTruthy();

    await send("oi");
    expect(screen.queryByText(/9\.9\.9 disponível/)).toBeNull();

    emit({ kind: "done", ok: true });
    await waitFor(() => expect(screen.getByLabelText("Enviar")).toBeTruthy());
    expect(screen.getByText(/9\.9\.9 disponível/)).toBeTruthy();
  });
});
