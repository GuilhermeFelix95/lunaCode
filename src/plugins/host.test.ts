import { afterEach, describe, expect, it, vi } from "vitest";
import { PLUGIN_TIMEOUT_MS, PluginHost, type PluginApi, type PluginManifest, type PluginModule } from "./host";

const manifest = (overrides: Partial<PluginManifest> = {}): PluginManifest => ({
  id: "hello",
  name: "Hello",
  version: "0.1.0",
  entry: "index.js",
  permissions: ["commands", "hooks"],
  ...overrides,
});

const plugin = (activate: PluginModule["activate"]) => async () => ({ activate });

describe("PluginHost", () => {
  it("registra comando e executa com os argumentos após a barra", async () => {
    const host = new PluginHost();
    await host.load(
      manifest(),
      plugin((api) => api.registerCommand("ola", (args) => `olá, ${args}`)),
    );

    expect(await host.handleCommand("/ola mundo")).toEqual({ handled: true, prompt: "olá, mundo" });
    expect(host.commandNames()).toEqual(["ola"]);
  });

  it("não ativa um plugin que estava carregando quando a lista foi recarregada", async () => {
    const host = new PluginHost();
    let finishLoading!: () => void;
    const loading = host.load(manifest(), async () => {
      await new Promise<void>((resolve) => (finishLoading = resolve));
      return { activate: (api) => api.registerCommand("ola", () => "x") };
    });

    // Recarregar (ex.: plugin desligado nas Configurações) enquanto a carga ainda não terminou.
    host.reset();
    finishLoading();

    expect(await loading).toBe(false);
    expect(host.commandNames()).toEqual([]);
    expect(host.loaded).toEqual([]);
  });

  it("não registra nada de uma ativação que terminou depois de um reset", async () => {
    const host = new PluginHost();
    let finishActivate!: () => void;
    const loading = host.load(manifest(), async () => ({
      activate: async (api) => {
        await new Promise<void>((resolve) => (finishActivate = resolve));
        api.registerCommand("ola", () => "x");
      },
    }));

    await vi.waitFor(() => expect(finishActivate).toBeTypeOf("function"));
    host.reset();
    finishActivate();

    expect(await loading).toBe(false);
    expect(host.commandNames()).toEqual([]);
    expect(host.errors).toEqual([]);
  });

  it("deixa a entrada seguir o fluxo normal quando não há comando", async () => {
    const host = new PluginHost();
    expect(await host.handleCommand("/desconhecido")).toEqual({ handled: false });
    expect(await host.handleCommand("texto comum")).toEqual({ handled: false });
  });

  it("comando que só executa ação não gera prompt", async () => {
    const host = new PluginHost();
    await host.load(manifest(), plugin((api) => api.registerCommand("limpa", () => undefined)));

    expect(await host.handleCommand("/limpa")).toEqual({ handled: true });
  });

  it("recusa registrar comando sem a permissão commands", async () => {
    const host = new PluginHost();
    const ok = await host.load(
      manifest({ permissions: ["hooks"] }),
      plugin((api) => api.registerCommand("ola", () => "x")),
    );

    expect(ok).toBe(false);
    expect(host.errors[0]).toContain('não tem a permissão "commands"');
    expect(host.commandNames()).toEqual([]);
  });

  it("recusa hooks sem a permissão hooks", async () => {
    const host = new PluginHost();
    const ok = await host.load(
      manifest({ permissions: ["commands"] }),
      plugin((api) => api.onBeforeSend((p) => p)),
    );

    expect(ok).toBe(false);
    expect(host.errors[0]).toContain('não tem a permissão "hooks"');
  });

  it("rejeita nome de comando inválido e comando duplicado", async () => {
    const host = new PluginHost();
    await host.load(
      manifest(),
      plugin((api) => {
        api.registerCommand("ola", () => "a");
        api.registerCommand("Ola", () => "b");
      }),
    );
    expect(host.errors[0]).toContain('nome de comando inválido "Ola"');

    // Ativação que falhou não deixa comandos para trás: "ola" ficou livre para outro plugin.
    expect(host.commandNames()).toEqual([]);
    await host.load(manifest({ id: "um" }), plugin((api) => api.registerCommand("ola", () => "c")));
    await host.load(manifest({ id: "outro" }), plugin((api) => api.registerCommand("ola", () => "d")));
    expect(host.errors[1]).toContain('comando "/ola" já existe');
  });

  it("encadeia hooks antes de enviar e ignora falha isolada", async () => {
    const host = new PluginHost();
    await host.load(
      manifest(),
      plugin((api) => {
        api.onBeforeSend((p) => `[ctx] ${p}`);
        api.onBeforeSend(() => {
          throw new Error("quebrou");
        });
        api.onBeforeSend((p) => `${p} [fim]`);
      }),
    );

    expect(await host.runBeforeSend("oi")).toBe("[ctx] oi [fim]");
    expect(host.errors).toEqual(["hello (antes de enviar): quebrou"]);
  });

  it("entrega o texto final da resposta aos hooks depois da resposta", async () => {
    const host = new PluginHost();
    const received: string[] = [];
    await host.load(manifest(), plugin((api) => api.onAfterResponse((t) => void received.push(t))));

    await host.runAfterResponse("resposta pronta");

    expect(received).toEqual(["resposta pronta"]);
  });

  it("falha ao carregar o módulo não impede os próximos plugins", async () => {
    const host = new PluginHost();
    const ok = await host.load(manifest({ id: "ruim" }), async () => {
      throw new Error("sintaxe inválida");
    });
    await host.load(manifest({ id: "bom" }), plugin((api) => api.registerCommand("bom", () => "ok")));

    expect(ok).toBe(false);
    expect(host.errors[0]).toBe("ruim: sintaxe inválida");
    expect(host.loaded).toEqual(["bom"]);
  });
});

describe("PluginHost: modos", () => {
  const mode = {
    id: "ask",
    label: "Conversar",
    description: "Só explica",
    instruction: "Só explique.",
    permission: "readOnly" as const,
  };

  it("registra um modo de somente leitura com a permissão modes", async () => {
    const host = new PluginHost();
    const ok = await host.load(manifest({ permissions: ["modes"] }), plugin((api) => api.registerMode(mode)));

    expect(ok).toBe(true);
    expect(host.modeList()).toEqual([mode]);
  });

  it("recusa modo sem a permissão modes", async () => {
    const host = new PluginHost();
    const ok = await host.load(manifest({ permissions: ["commands"] }), plugin((api) => api.registerMode(mode)));

    expect(ok).toBe(false);
    expect(host.errors[0]).toContain('não tem a permissão "modes"');
    expect(host.modeList()).toEqual([]);
  });

  it("nunca aceita modo que dê mais permissão do que o composer tem", async () => {
    const host = new PluginHost();
    const ok = await host.load(
      manifest({ permissions: ["modes"] }),
      plugin((api) => api.registerMode({ ...mode, permission: "fullAccess" as never })),
    );

    expect(ok).toBe(false);
    expect(host.errors[0]).toContain('precisa usar permission "readOnly"');
    expect(host.modeList()).toEqual([]);
  });

  it("rejeita id de modo repetido e reset esvazia modos, comandos e erros", async () => {
    const host = new PluginHost();
    await host.load(
      manifest({ permissions: ["modes", "commands"] }),
      plugin((api) => {
        api.registerMode(mode);
        api.registerMode(mode);
        api.registerCommand("ola", () => "x");
      }),
    );
    expect(host.errors[0]).toContain('modo "ask" já existe');

    host.reset();
    expect(host.modeList()).toEqual([]);
    expect(host.commandNames()).toEqual([]);
    expect(host.errors).toEqual([]);
  });
});

describe("PluginHost: limites e ativação parcial", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("um hook que não responde é cortado no tempo limite e o texto segue sem ele", async () => {
    vi.useFakeTimers();
    const host = new PluginHost();
    await host.load(
      manifest(),
      plugin((api) => api.onBeforeSend(() => new Promise<string>(() => undefined))),
    );

    const pending = host.runBeforeSend("oi");
    await vi.advanceTimersByTimeAsync(PLUGIN_TIMEOUT_MS);

    expect(await pending).toBe("oi");
    expect(host.errors[0]).toContain("antes de enviar");
    expect(host.errors[0]).toContain("passou de 10 s");
  });

  it("um comando que não responde também é cortado", async () => {
    vi.useFakeTimers();
    const host = new PluginHost();
    await host.load(
      manifest(),
      plugin((api) => api.registerCommand("trava", () => new Promise<string>(() => undefined))),
    );

    const pending = host.handleCommand("/trava");
    await vi.advanceTimersByTimeAsync(PLUGIN_TIMEOUT_MS);

    expect(await pending).toEqual({ handled: true });
    expect(host.errors[0]).toContain("/trava");
  });

  it("se activate falha no meio, o que já foi registrado é desfeito", async () => {
    const host = new PluginHost();
    const ok = await host.load(
      manifest(),
      plugin((api) => {
        api.registerCommand("meio", () => "x");
        api.onBeforeSend((p) => p);
        throw new Error("falhou depois de registrar");
      }),
    );

    expect(ok).toBe(false);
    expect(host.commandNames()).toEqual([]);
    expect(await host.handleCommand("/meio")).toEqual({ handled: false });
    expect(host.errors[0]).toContain("falhou depois de registrar");
  });

  it("modo declara se quer perguntas, sem depender do id", async () => {
    const host = new PluginHost();
    await host.load(
      manifest({ permissions: ["modes"] }),
      plugin((api) =>
        api.registerMode({
          id: "revisao",
          label: "Revisão",
          description: "Pergunta antes de agir",
          instruction: "Pergunte antes de alterar arquivos.",
          permission: "readOnly",
          questions: true,
        }),
      ),
    );

    expect(host.modeList()[0].questions).toBe(true);
  });
});

describe("PluginHost: cache", () => {
  const resumo = {
    session: 0.5,
    readTotal: 10,
    freshTotal: 10,
    writeTotal: 0,
    last: null,
    history: [0.5],
    warm: { state: "sem-dados" as const },
    tip: null,
  };

  it("recusa api.cache sem a permissão cache", async () => {
    const host = new PluginHost();
    const ok = await host.load(manifest({ permissions: ["commands"] }), plugin((api) => void api.cache.stats()));

    expect(ok).toBe(false);
    expect(host.errors[0]).toContain('não tem a permissão "cache"');
  });

  it("stats devolve null antes do primeiro turno e o resumo da conversa depois", async () => {
    const host = new PluginHost();
    let api!: PluginApi;
    await host.load(manifest({ permissions: ["cache"] }), plugin((a) => { api = a; }));

    expect(api.cache.stats()).toBeNull();
    host.setCacheSummary(resumo);
    expect(api.cache.stats()).toEqual(resumo);
  });

  it("a API é só leitura: não expõe ações que liguem, desliguem ou limpem o cache", async () => {
    const host = new PluginHost();
    let api!: PluginApi;
    await host.load(manifest({ permissions: ["cache"] }), plugin((a) => { api = a; }));

    expect("setEnabled" in api.cache).toBe(false);
    expect("clear" in api.cache).toBe(false);
  });

  it("notify vira aviso local, sem ir ao agente", async () => {
    const host = new PluginHost();
    await host.load(manifest({ permissions: ["cache"] }), plugin((api) => api.cache.notify("cache ok")));

    expect(host.messages()).toContain("cache ok");
  });
});

describe("PluginHost: ui", () => {
  afterEach(() => {
    document.body.innerHTML = "";
    document.head.innerHTML = "";
  });

  it("recusa api.ui sem a permissão ui", async () => {
    const host = new PluginHost();
    const ok = await host.load(manifest({ permissions: ["commands"] }), plugin((api) => api.ui.addStyle("p{}")));

    expect(ok).toBe(false);
    expect(host.errors[0]).toContain('não tem a permissão "ui"');
  });

  it("addStyle injeta a folha e reset a remove", async () => {
    const host = new PluginHost();
    await host.load(manifest({ permissions: ["ui"] }), plugin((api) => api.ui.addStyle(".x{color:red}")));

    expect(document.head.querySelector('style[data-plugin="hello"]')?.textContent).toBe(".x{color:red}");
    host.reset();
    expect(document.head.querySelector('style[data-plugin="hello"]')).toBeNull();
  });

  it("mount monta no ponto quando ele existe, e reset desmonta com o cleanup", async () => {
    const slot = document.createElement("div");
    slot.dataset.pluginSlot = "sidebar";
    document.body.appendChild(slot);
    const cleanup = vi.fn();
    const host = new PluginHost();
    await host.load(
      manifest({ permissions: ["ui"] }),
      plugin((api) =>
        api.ui.mount("sidebar", (el) => {
          el.textContent = "olá";
          return cleanup;
        }),
      ),
    );

    expect(slot.querySelector('[data-plugin="hello"]')?.textContent).toBe("olá");
    host.reset();
    expect(cleanup).toHaveBeenCalledTimes(1);
    expect(slot.querySelector('[data-plugin="hello"]')).toBeNull();
  });

  it("mount espera o ponto aparecer na tela", async () => {
    const host = new PluginHost();
    await host.load(
      manifest({ permissions: ["ui"] }),
      plugin((api) => api.ui.mount("chat", (el) => void (el.textContent = "chat"))),
    );
    expect(document.querySelector('[data-plugin="hello"]')).toBeNull();

    const slot = document.createElement("div");
    slot.dataset.pluginSlot = "chat";
    document.body.appendChild(slot);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(slot.querySelector('[data-plugin="hello"]')?.textContent).toBe("chat");
    host.reset();
    expect(slot.querySelector('[data-plugin="hello"]')).toBeNull();
  });
});

describe("PluginHost: um bloco por plugin e por ponto", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("duas conversas com o mesmo plugin mostram um bloco; o outro assume quando o primeiro fecha", async () => {
    const slot = document.createElement("div");
    slot.dataset.pluginSlot = "sidebar";
    document.body.appendChild(slot);
    const mount = plugin((api) => api.ui.mount("sidebar", (el) => { el.textContent = "painel"; }));
    const primeira = new PluginHost();
    const segunda = new PluginHost();
    await primeira.load(manifest({ permissions: ["ui"] }), mount);
    await segunda.load(manifest({ permissions: ["ui"] }), mount);

    expect(slot.querySelectorAll('[data-plugin="hello"]').length).toBe(1);

    primeira.reset();
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(slot.querySelectorAll('[data-plugin="hello"]').length).toBe(1);
    expect(slot.textContent).toContain("painel");
  });
});

describe("PluginHost: temas de plugin", () => {
  const theme = {
    id: "verde",
    name: "Verde",
    dark: true,
    tokens: { "--bg": "#9bbc0f", "--fg": "#0f380f" },
    css: ".chat { image-rendering: pixelated; }",
  };

  it("registra um tema com a permissão theme", async () => {
    const host = new PluginHost();
    const ok = await host.load(manifest({ permissions: ["theme"] }), plugin((api) => api.theme.register(theme)));

    expect(ok).toBe(true);
    expect(host.themeList()).toEqual([{ pluginId: "hello", ...theme }]);
  });

  it("recusa tema sem a permissão theme", async () => {
    const host = new PluginHost();
    const ok = await host.load(manifest({ permissions: ["commands"] }), plugin((api) => api.theme.register(theme)));

    expect(ok).toBe(false);
    expect(host.errors[0]).toContain('não tem a permissão "theme"');
    expect(host.themeList()).toEqual([]);
  });

  it("recusa token que não é variável CSS ou que tenta sair do valor", async () => {
    const host = new PluginHost();
    const ok = await host.load(
      manifest({ permissions: ["theme"] }),
      plugin((api) => api.theme.register({ ...theme, tokens: { "--bg": "red; background: url(x)" } })),
    );

    expect(ok).toBe(false);
    expect(host.themeList()).toEqual([]);
  });

  it("some com os temas quando o plugin é recarregado", async () => {
    const host = new PluginHost();
    await host.load(manifest({ permissions: ["theme"] }), plugin((api) => api.theme.register(theme)));
    host.reset();

    expect(host.themeList()).toEqual([]);
  });

  it("aceita mascote e som de resposta só como asset(assets/...)", async () => {
    const host = new PluginHost();
    const ok = await host.load(
      manifest({ permissions: ["theme"] }),
      plugin((api) =>
        api.theme.register({ ...theme, id: "midia", mascot: "asset(assets/luna.png)", sounds: { reply: "asset(assets/ding.ogg)" } }),
      ),
    );

    expect(ok).toBe(true);
    expect(host.themeList()[0].mascot).toBe("asset(assets/luna.png)");
    expect(host.themeList()[0].sounds).toEqual({ reply: "asset(assets/ding.ogg)" });
  });

  it("recusa mascote que não é uma referência de asset", async () => {
    const host = new PluginHost();
    const ok = await host.load(
      manifest({ permissions: ["theme"] }),
      plugin((api) => api.theme.register({ ...theme, id: "ruim", mascot: "https://exemplo.com/luna.png" })),
    );

    expect(ok).toBe(false);
    expect(host.themeList()).toEqual([]);
  });
});

describe("onTurnDone (LunaFix)", () => {
  const turn = { text: "pronto", ok: true, lastCommand: { ok: false, output: "Exit code 1" }, attempt: 0 };

  it("exige a permissão autofix", async () => {
    const host = new PluginHost();
    const loaded = await host.load(
      manifest({ permissions: ["hooks"] }),
      plugin((api) => api.onTurnDone(() => "corrija")),
    );

    expect(loaded).toBe(false);
    expect(host.errors.join(" ")).toContain('"autofix"');
  });

  it("devolve o prompt do primeiro plugin que pede correção, com o resultado do último comando", async () => {
    const host = new PluginHost();
    const seen: unknown[] = [];
    await host.load(
      manifest({ id: "lunafix", permissions: ["autofix"] }),
      plugin((api) =>
        api.onTurnDone((info) => {
          seen.push(info);
          return info.lastCommand && !info.lastCommand.ok ? `corrija: ${info.lastCommand.output}` : undefined;
        }),
      ),
    );

    expect(await host.runTurnDone(turn)).toBe("corrija: Exit code 1");
    expect(seen).toEqual([turn]);
  });

  it("sem hook, não devolve nada; um hook que falha não segura a entrega", async () => {
    const host = new PluginHost();
    expect(await host.runTurnDone(turn)).toBeNull();

    await host.load(
      manifest({ id: "quebrado", permissions: ["autofix"] }),
      plugin((api) =>
        api.onTurnDone(() => {
          throw new Error("falha");
        }),
      ),
    );

    expect(await host.runTurnDone(turn)).toBeNull();
    expect(host.errors.join(" ")).toContain("falha");
  });
});
