import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../../api";
import { BrowserPanel } from "./BrowserPanel";

// Substitui a ponte com o Rust: o teste só confere o que o painel pede.
vi.mock("../../api", () => ({
  browserShow: vi.fn(async () => {}),
  browserSetBounds: vi.fn(async () => {}),
  browserNewTab: vi.fn(async () => 1),
  browserSwitchTab: vi.fn(async () => {}),
  browserCloseTab: vi.fn(async () => null),
  browserSetEconomy: vi.fn(async () => {}),
  browserSetAdblock: vi.fn(async () => {}),
  browserSetProject: vi.fn(async () => {}),
  browserTranslate: vi.fn(async (on: boolean) => (on ? "https://translate.google.com/translate?sl=auto&tl=pt&u=https%3A%2F%2Fexample.com%2F" : "https://example.com/")),
  browserState: vi.fn(async () => ({ tabs: [], active: null })),
  browserNavigate: vi.fn(async (url: string) => `https://${url}/`),
  browserHistory: vi.fn(async () => {}),
  browserReload: vi.fn(async () => {}),
  browserBookmarks: vi.fn(async () => []),
  browserBookmarkAdd: vi.fn(async () => []),
  browserBookmarkRemove: vi.fn(async () => []),
  onBrowserOpened: vi.fn(async () => () => {}),
  onBrowserNavigated: vi.fn(async () => () => {}),
  onBrowserBookmarksChanged: vi.fn(async () => () => {}),
  onBrowserShortcut: vi.fn(async () => () => {}),
  browserLoginSuggestions: vi.fn(async () => [] as string[]),
  browserFillLogin: vi.fn(async () => {}),
  browserFavicon: vi.fn(async () => null),
  browserVisits: vi.fn(async () => [] as { url: string; visitedAt: number }[]),
}));

const AREA = { x: 12, y: 40, width: 300, height: 500, left: 12, top: 40, right: 312, bottom: 540, toJSON: () => ({}) };
const BOUNDS = { x: 12, y: 40, width: 300, height: 500 };

/// Botão pelo nome acessível; o jsdom não tem as asserções do jest-dom neste projeto.
function button(name: string) {
  return screen.getByRole("button", { name }) as HTMLButtonElement;
}

function submit(text: string) {
  fireEvent.change(screen.getByLabelText("Endereço"), { target: { value: text } });
  fireEvent.submit(screen.getByLabelText("Endereço").closest("form")!);
}

function withTabs(tabs: { id: number; url: string }[], active: number | null) {
  vi.mocked(api.browserState).mockResolvedValue({ tabs, active, adblock: true });
}

describe("BrowserPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.browserState).mockResolvedValue({ tabs: [], active: null, adblock: true });
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
  });

  it("a estrela de favorito acende quando a página muda sozinha (clique num link)", async () => {
    let navigated: () => void = () => {};
    vi.mocked(api.onBrowserNavigated).mockImplementation(async (handler) => {
      navigated = handler;
      return () => {};
    });
    withTabs([{ id: 1, url: "" }], 1);
    render(<BrowserPanel active />);
    await waitFor(() => expect(api.browserState).toHaveBeenCalled());
    expect(button("Salvar nos favoritos").disabled).toBe(true);

    // A pessoa clica num link dentro da página: a aba agora tem endereço, e o painel precisa ler de novo.
    withTabs([{ id: 1, url: "https://exemplo.com/" }], 1);
    act(() => navigated());

    await waitFor(() => expect(button("Salvar nos favoritos").disabled).toBe(false));
    fireEvent.click(button("Salvar nos favoritos"));
    await waitFor(() => expect(api.browserBookmarkAdd).toHaveBeenCalledWith("https://exemplo.com/", expect.any(String)));
  });

  it("sem aba aberta, explica o que fazer e não deixa voltar", async () => {
    render(<BrowserPanel active />);

    await waitFor(() => expect(api.browserState).toHaveBeenCalled());
    expect(screen.getByText(/Digite um endereço acima/)).toBeTruthy();
    expect(button("Voltar").disabled).toBe(true);
  });

  it("enviar um endereço sem aba abre a primeira aba com ele", async () => {
    render(<BrowserPanel active />);
    await waitFor(() => expect(api.browserState).toHaveBeenCalled());

    submit("exemplo.com");

    await waitFor(() => expect(api.browserNewTab).toHaveBeenCalledWith("exemplo.com", BOUNDS));
    expect(api.browserNavigate).not.toHaveBeenCalled();
  });

  it("com uma aba ativa, um endereço novo navega a aba atual", async () => {
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
    render(<BrowserPanel active />);
    await waitFor(() => expect(button("Voltar").disabled).toBe(false));

    submit("outro.com");

    await waitFor(() => expect(api.browserNavigate).toHaveBeenCalledWith("outro.com"));
    expect(api.browserNewTab).not.toHaveBeenCalled();
    expect((screen.getByLabelText("Endereço") as HTMLInputElement).value).toBe("https://outro.com/");
  });

  it("nova aba em branco fica ativa e aparece na barra", async () => {
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
    vi.mocked(api.browserNewTab).mockResolvedValueOnce(2);
    render(<BrowserPanel active />);
    await waitFor(() => expect(screen.getByRole("tab", { name: "a.com" })).toBeTruthy());

    fireEvent.click(button("Nova aba"));

    await waitFor(() => expect(api.browserNewTab).toHaveBeenCalledWith(null, BOUNDS));
    expect(screen.getByRole("tab", { name: "Nova aba" }).getAttribute("aria-selected")).toBe("true");
  });

  it("com sete abas, o botão de nova aba ainda está liberado", async () => {
    withTabs(
      [1, 2, 3, 4, 5, 6, 7].map((id) => ({ id, url: `https://site${id}.com/` })),
      1,
    );
    render(<BrowserPanel active />);

    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(7));
    expect(button("Nova aba").disabled).toBe(false);
  });

  it("clicar numa aba troca a ativa", async () => {
    withTabs(
      [
        { id: 1, url: "https://a.com/" },
        { id: 2, url: "https://b.com/" },
      ],
      1,
    );
    render(<BrowserPanel active />);
    await waitFor(() => expect(screen.getByRole("tab", { name: "b.com" })).toBeTruthy());

    fireEvent.click(screen.getByRole("tab", { name: "b.com" }));

    await waitFor(() => expect(api.browserSwitchTab).toHaveBeenCalledWith(2, BOUNDS));
    expect(screen.getByRole("tab", { name: "b.com" }).getAttribute("aria-selected")).toBe("true");
  });

  it("fechar uma aba mantém as outras e a aba que o Rust escolheu como ativa", async () => {
    withTabs(
      [
        { id: 1, url: "https://a.com/" },
        { id: 2, url: "https://b.com/" },
      ],
      2,
    );
    vi.mocked(api.browserCloseTab).mockResolvedValueOnce(1);
    render(<BrowserPanel active />);
    await waitFor(() => expect(screen.getByRole("tab", { name: "b.com" })).toBeTruthy());

    fireEvent.click(button("Fechar b.com"));

    await waitFor(() => expect(api.browserCloseTab).toHaveBeenCalledWith(2));
    await waitFor(() => expect(screen.queryByRole("tab", { name: "b.com" })).toBeNull());
    expect(screen.getByRole("tab", { name: "a.com" }).getAttribute("aria-selected")).toBe("true");
  });

  it("salvar a página atual nos favoritos guarda o endereço e mostra o favorito na lista", async () => {
    withTabs([{ id: 1, url: "https://vps.exemplo.com/" }], 1);
    vi.mocked(api.browserBookmarkAdd).mockResolvedValueOnce([{ id: "x", title: "vps.exemplo.com", url: "https://vps.exemplo.com/" }]);
    render(<BrowserPanel active />);
    await waitFor(() => expect(button("Salvar nos favoritos").disabled).toBe(false));

    fireEvent.click(button("Salvar nos favoritos"));

    await waitFor(() => expect(api.browserBookmarkAdd).toHaveBeenCalledWith("https://vps.exemplo.com/", "vps.exemplo.com"));
    expect(await screen.findByRole("list", { name: "Favoritos" })).toBeTruthy();
    expect(button("Tirar dos favoritos")).toBeTruthy();
  });

  it("favoritos importados aparecem na barra sem reabrir o painel", async () => {
    let refresh: () => void = () => {};
    vi.mocked(api.onBrowserBookmarksChanged).mockImplementation(async (handler) => {
      refresh = handler;
      return () => {};
    });
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
    render(<BrowserPanel active />);
    await waitFor(() => expect(api.browserBookmarks).toHaveBeenCalled());
    expect((await screen.findByRole("list", { name: "Favoritos" })).textContent).toContain("Nenhum favorito salvo ainda");

    // A importação terminou em outra tela: a lista guardada mudou e o painel precisa ler de novo.
    vi.mocked(api.browserBookmarks).mockResolvedValue([{ id: "f1", title: "Importado", url: "https://importado.com/" }]);
    act(() => refresh());

    expect(await screen.findByRole("button", { name: "Importado" })).toBeTruthy();
  });

  it("a barra de favoritos fica à vista mesmo sem favoritos, com um aviso", async () => {
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
    render(<BrowserPanel active />);

    expect((await screen.findByRole("list", { name: "Favoritos" })).textContent).toContain("Nenhum favorito salvo ainda");
  });

  it("abrir um favorito navega a aba atual até a página dele", async () => {
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
    vi.mocked(api.browserBookmarks).mockResolvedValueOnce([{ id: "x", title: "VPS", url: "https://vps.exemplo.com/" }]);
    render(<BrowserPanel active />);
    const list = within(await screen.findByRole("list", { name: "Favoritos" }));

    fireEvent.click(list.getByRole("button", { name: "VPS" }));

    await waitFor(() => expect(api.browserNavigate).toHaveBeenCalledWith("https://vps.exemplo.com/"));
  });

  it("ao sair da aba, a página some; ao voltar, aparece de novo", async () => {
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
    const { rerender } = render(<BrowserPanel active={false} />);
    await waitFor(() => expect(api.browserShow).toHaveBeenCalledWith(false));

    rerender(<BrowserPanel active />);
    await waitFor(() => expect(api.browserShow).toHaveBeenLastCalledWith(true));
  });

  it("um endereço recusado mostra o motivo para a pessoa", async () => {
    vi.mocked(api.browserNewTab).mockRejectedValueOnce(new Error("só http e https são permitidos, não \"file\""));
    render(<BrowserPanel active />);
    await waitFor(() => expect(api.browserState).toHaveBeenCalled());

    submit("file:///C:/segredo.txt");

    expect((await screen.findByRole("alert")).textContent).toContain("só http e https são permitidos");
  });

  it("voltar e avançar pedem ao navegador o histórico da aba ativa", async () => {
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
    render(<BrowserPanel active />);
    await waitFor(() => expect(button("Voltar").disabled).toBe(false));

    fireEvent.click(button("Voltar"));
    fireEvent.click(button("Avançar"));

    expect(api.browserHistory).toHaveBeenNthCalledWith(1, "back");
    expect(api.browserHistory).toHaveBeenNthCalledWith(2, "forward");
  });

  it("página aberta pelo agente aparece no painel sem clique", async () => {
    let fire: (url: string) => void = () => {};
    vi.mocked(api.onBrowserOpened).mockImplementationOnce(async (handler) => {
      fire = handler;
      return () => {};
    });
    withTabs([{ id: 1, url: "https://exemplo.com/" }], 1);
    render(<BrowserPanel active={false} />);
    await act(async () => fire("https://exemplo.com/"));
    await waitFor(() => expect((screen.getByLabelText("Endereço") as HTMLInputElement).value).toBe("https://exemplo.com/"));
  });
});

describe("Pesquisa pela barra de endereço", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(api.browserState).mockResolvedValue({ tabs: [], active: null, adblock: true });
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
  });

  function start() {
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
    render(<BrowserPanel active />);
    return waitFor(() => expect(button("Voltar").disabled).toBe(false));
  }

  it("texto que não é endereço vira busca no motor padrão (Google)", async () => {
    await start();

    submit("gatos fofos");

    await waitFor(() => expect(api.browserNavigate).toHaveBeenCalledWith("https://www.google.com/search?q=gatos%20fofos"));
  });

  it("endereço (domínio ou esquema) navega direto, sem virar busca", async () => {
    await start();

    submit("exemplo.com");

    await waitFor(() => expect(api.browserNavigate).toHaveBeenCalledWith("exemplo.com"));
    await waitFor(() => expect(api.browserNavigate).not.toHaveBeenCalledWith(expect.stringContaining("google.com")));
  });

  it("com o motor trocado nas configurações, a busca segue o motor escolhido", async () => {
    localStorage.setItem("luna.searchEngine", "duckduckgo");
    await start();

    submit("gatos fofos");

    await waitFor(() => expect(api.browserNavigate).toHaveBeenCalledWith("https://duckduckgo.com/?q=gatos%20fofos"));
  });

  it("o histórico guarda visitas recentes e uma delas navega ao clicar", async () => {
    vi.mocked(api.browserVisits).mockResolvedValue([
      { url: "https://mais-recente.com/", visitedAt: 10 },
      { url: "https://exemplo.com/", visitedAt: 2 },
    ]);
    await start();

    fireEvent.click(button("Histórico"));

    const dropdown = await screen.findByRole("list", { name: "Visitas recentes" });
    const entries = within(dropdown).getAllByRole("button").map((b) => b.textContent);
    expect(entries).toEqual(["mais-recente.com", "exemplo.com"]);

    fireEvent.click(within(dropdown).getByRole("button", { name: "exemplo.com" }));
    await waitFor(() => expect(api.browserNavigate).toHaveBeenCalledWith("https://exemplo.com/"));
  });
});

describe("BrowserPanel: área acompanha o painel por quadro", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("vários ajustes no mesmo quadro viram um só pedido ao Rust", async () => {
    const frames: FrameRequestCallback[] = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => frames.push(cb));
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
    vi.mocked(api.browserState).mockResolvedValue({ tabs: [{ id: 1, url: "https://example.com/" }], active: 1, adblock: true });

    render(<BrowserPanel active openRequest={null} />);
    await waitFor(() => expect(frames.length).toBeGreaterThan(0));
    act(() => frames.splice(0).forEach((cb) => cb(0)));
    vi.mocked(api.browserSetBounds).mockClear();

    window.dispatchEvent(new Event("resize"));
    window.dispatchEvent(new Event("resize"));
    window.dispatchEvent(new Event("resize"));
    expect(api.browserSetBounds).not.toHaveBeenCalled();

    act(() => frames.splice(0).forEach((cb) => cb(0)));
    expect(api.browserSetBounds).toHaveBeenCalledTimes(1);
    expect(api.browserSetBounds).toHaveBeenCalledWith(BOUNDS);
  });
});

describe("BrowserPanel: controles na faixa de abas", () => {
  it("os controles de início e de fim ficam na mesma faixa das abas, fora da lista de abas", async () => {
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    render(
      <BrowserPanel active openRequest={null} toolbarStart={<button type="button">Modo LunaCode</button>} toolbarEnd={<button type="button">Mostrar o chat</button>} />,
    );

    const start = screen.getByRole("button", { name: "Modo LunaCode" });
    const end = screen.getByRole("button", { name: "Mostrar o chat" });
    expect(start.closest('[role="tablist"]')).toBeNull();
    expect(end.closest('[role="tablist"]')).toBeNull();
    expect(start.parentElement).toBe(end.parentElement);
    vi.unstubAllGlobals();
  });
});

describe("BrowserPanel: economia de memória e limite de abas", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("cada aba tem um botão que liga e desliga a economia de memória, com o estado anunciado", async () => {
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
    render(<BrowserPanel active openRequest={null} />);

    const toggle = await screen.findByRole("button", { name: "Economizar memória de a.com" });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");

    fireEvent.click(toggle);

    await waitFor(() => expect(api.browserSetEconomy).toHaveBeenCalledWith(1, true));
    expect(screen.getByRole("button", { name: "Economizar memória de a.com" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("aceita até 8 abas: com oito abertas, o botão de nova aba fica desabilitado", async () => {
    withTabs(
      Array.from({ length: 8 }, (_, i) => ({ id: i + 1, url: `https://site${i + 1}.com/` })),
      1,
    );
    render(<BrowserPanel active openRequest={null} />);

    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(8));
    expect(button("Nova aba").disabled).toBe(true);
  });
});

describe("BrowserPanel: bloqueador de anúncios", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("vem ligado e pode ser pausado e religado, com o estado anunciado", async () => {
    vi.mocked(api.browserState).mockResolvedValue({ tabs: [], active: null, adblock: true });
    render(<BrowserPanel active openRequest={null} />);

    const on = await screen.findByRole("button", { name: "Bloqueador de anúncios ligado" });
    expect(on.getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(on);

    await waitFor(() => expect(api.browserSetAdblock).toHaveBeenCalledWith(false));
    expect(screen.getByRole("button", { name: "Bloqueador de anúncios pausado" }).getAttribute("aria-pressed")).toBe("false");
  });
});

describe("BrowserPanel: tradução da página", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("traduz a página atual e volta ao original quando pausada, com o estado anunciado", async () => {
    withTabs([{ id: 1, url: "https://example.com/" }], 1);
    render(<BrowserPanel active openRequest={null} />);

    fireEvent.click(await screen.findByRole("button", { name: "Traduzir página para português" }));
    await waitFor(() => expect(api.browserTranslate).toHaveBeenCalledWith(true));
    const back = await screen.findByRole("button", { name: "Mostrar página original" });
    expect(back.getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(back);
    await waitFor(() => expect(api.browserTranslate).toHaveBeenLastCalledWith(false));
    expect(await screen.findByRole("button", { name: "Traduzir página para português" })).toBeTruthy();
  });
});

describe("BrowserPanel: busca no histórico", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
    withTabs([{ id: 1, url: "https://a.com/" }], 1);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("mostra só as visitas cujo endereço contém o texto digitado", async () => {
    vi.mocked(api.browserVisits).mockResolvedValue([
      { url: "https://mais-recente.com/", visitedAt: 10 },
      { url: "https://exemplo.com/artigo", visitedAt: 2 },
    ]);
    render(<BrowserPanel active openRequest={null} />);
    fireEvent.click(await screen.findByRole("button", { name: "Histórico" }));

    fireEvent.change(await screen.findByRole("searchbox", { name: "Buscar no histórico" }), { target: { value: "ARTIGO" } });

    const dropdown = screen.getByRole("list", { name: "Visitas recentes" });
    expect(within(dropdown).getAllByRole("button").map((b) => b.textContent)).toEqual(["exemplo.com"]);
  });

  it("a busca mostra no máximo 50 visitas, mesmo que mais batam", async () => {
    vi.mocked(api.browserVisits).mockResolvedValue(Array.from({ length: 60 }, (_, i) => ({ url: `https://site${i}.com/`, visitedAt: i })));
    render(<BrowserPanel active openRequest={null} />);
    fireEvent.click(await screen.findByRole("button", { name: "Histórico" }));

    fireEvent.change(await screen.findByRole("searchbox", { name: "Buscar no histórico" }), { target: { value: "site" } });

    const dropdown = screen.getByRole("list", { name: "Visitas recentes" });
    expect(within(dropdown).getAllByRole("button")).toHaveLength(50);
  });

  it("sem visita que bata com o texto, diz isso", async () => {
    vi.mocked(api.browserVisits).mockResolvedValue([{ url: "https://exemplo.com/", visitedAt: 1 }]);
    render(<BrowserPanel active openRequest={null} />);
    fireEvent.click(await screen.findByRole("button", { name: "Histórico" }));

    fireEvent.change(await screen.findByRole("searchbox", { name: "Buscar no histórico" }), { target: { value: "nada-disso" } });

    expect(await screen.findByText("Nenhuma visita com esse texto.")).toBeTruthy();
  });
});

describe("BrowserPanel: atalhos do teclado", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
    withTabs(
      [
        { id: 1, url: "https://a.com/" },
        { id: 2, url: "https://b.com/" },
      ],
      1,
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("Ctrl+T abre uma aba nova em branco", async () => {
    render(<BrowserPanel active openRequest={null} />);
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(2));

    fireEvent.keyDown(window, { key: "t", ctrlKey: true });

    await waitFor(() => expect(api.browserNewTab).toHaveBeenCalledWith(null, BOUNDS));
  });

  it("Ctrl+W fecha a aba atual", async () => {
    render(<BrowserPanel active openRequest={null} />);
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(2));

    fireEvent.keyDown(window, { key: "w", ctrlKey: true });

    await waitFor(() => expect(api.browserCloseTab).toHaveBeenCalledWith(1));
  });

  it("Ctrl+L leva o foco para a barra de endereço", async () => {
    render(<BrowserPanel active openRequest={null} />);
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(2));

    fireEvent.keyDown(window, { key: "l", ctrlKey: true });

    expect(document.activeElement).toBe(screen.getByLabelText("Endereço"));
  });

  it("clicar na barra de endereço sem foco seleciona o endereço inteiro, e a seleção fica depois do clique", async () => {
    render(<BrowserPanel active openRequest={null} />);
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(2));
    const input = screen.getByLabelText("Endereço") as HTMLInputElement;
    await waitFor(() => expect(input.value).toBe("https://a.com/"));

    // Ordem de um clique real: o mousedown vem antes do foco. Se ele não for cancelado, o navegador põe
    // o cursor depois do foco e desfaz a seleção que o foco fez.
    expect(fireEvent.mouseDown(input)).toBe(false);
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe(input.value.length);
    expect(fireEvent.mouseUp(input)).toBe(true);
    expect(input.selectionEnd).toBe(input.value.length);
  });

  it("depois do foco pelo teclado, um clique comum no endereço não é cancelado (o cursor pode ser posto)", async () => {
    render(<BrowserPanel active openRequest={null} />);
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(2));
    const input = screen.getByLabelText("Endereço") as HTMLInputElement;
    await waitFor(() => expect(input.value).toBe("https://a.com/"));

    fireEvent.focus(input);
    fireEvent.mouseDown(input);
    expect(fireEvent.mouseUp(input)).toBe(true);
  });

  it("Ctrl+F abre a barra flutuante no lugar da busca da página e leva o endereço digitado para a aba", async () => {
    render(<BrowserPanel active openRequest={null} />);
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(2));

    // Quem chama vê que o atalho foi cancelado: a busca nativa da página não abre.
    expect(fireEvent.keyDown(window, { key: "f", ctrlKey: true })).toBe(false);
    const spotlight = screen.getByRole("dialog", { name: "Ir para endereço ou pesquisar" });
    fireEvent.change(within(spotlight).getByRole("textbox"), { target: { value: "exemplo.com" } });
    fireEvent.submit(within(spotlight).getByRole("textbox").closest("form")!);

    await waitFor(() => expect(api.browserNavigate).toHaveBeenCalledWith("exemplo.com"));
    expect(screen.queryByRole("dialog", { name: "Ir para endereço ou pesquisar" })).toBeNull();
  });

  it("o atalho que a página repassa pelo Rust faz o mesmo que o teclado", async () => {
    let shortcut: (key: string) => void = () => {};
    vi.mocked(api.onBrowserShortcut).mockImplementation(async (handler) => {
      shortcut = handler;
      return () => {};
    });
    render(<BrowserPanel active openRequest={null} />);
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(2));

    act(() => shortcut("t"));

    await waitFor(() => expect(api.browserNewTab).toHaveBeenCalledWith(null, BOUNDS));
  });

  it("atalhos não agem quando o painel do navegador está inativo", async () => {
    render(<BrowserPanel active={false} openRequest={null} />);
    await waitFor(() => expect(api.browserState).toHaveBeenCalled());

    fireEvent.keyDown(window, { key: "t", ctrlKey: true });

    expect(api.browserNewTab).not.toHaveBeenCalled();
  });
});

describe("BrowserPanel: login salvo", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
    withTabs([{ id: 1, url: "https://github.com/login" }], 1);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("nada é preenchido sem clique: a pessoa escolhe o login na lista", async () => {
    vi.mocked(api.browserLoginSuggestions).mockResolvedValue(["ana"]);
    render(<BrowserPanel active openRequest={null} />);
    await waitFor(() => expect(api.browserState).toHaveBeenCalled());
    expect(api.browserFillLogin).not.toHaveBeenCalled();

    fireEvent.click(await screen.findByRole("button", { name: "Preencher login salvo" }));
    fireEvent.click(await screen.findByRole("button", { name: "ana" }));

    await waitFor(() => expect(api.browserFillLogin).toHaveBeenCalledWith("ana"));
  });

  it("sem login salvo para a página, diz isso e não preenche nada", async () => {
    vi.mocked(api.browserLoginSuggestions).mockResolvedValue([]);
    render(<BrowserPanel active openRequest={null} />);

    fireEvent.click(await screen.findByRole("button", { name: "Preencher login salvo" }));

    expect(await screen.findByText("Nenhum login salvo para esta página.")).toBeTruthy();
    expect(api.browserFillLogin).not.toHaveBeenCalled();
  });
});

describe("BrowserPanel: painel inativo não esconde a página do painel ativo", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.browserState).mockResolvedValue({ tabs: [{ id: 1, url: "https://a.com/" }], active: 1, adblock: true });
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
  });

  it("uma conversa inativa montada depois da ativa deixa a página à vista", async () => {
    render(
      <>
        <BrowserPanel active />
        <BrowserPanel active={false} />
      </>,
    );

    await waitFor(() => expect(api.browserShow).toHaveBeenCalled());
    expect(vi.mocked(api.browserShow).mock.calls.at(-1)).toEqual([true]);
  });

  it("trocar de conversa esconde a página de quem saiu e mostra a de quem entrou", async () => {
    const { rerender } = render(
      <>
        <BrowserPanel active />
        <BrowserPanel active={false} />
      </>,
    );
    await waitFor(() => expect(api.browserShow).toHaveBeenCalled());

    rerender(
      <>
        <BrowserPanel active={false} />
        <BrowserPanel active />
      </>,
    );

    await waitFor(() => expect(vi.mocked(api.browserShow).mock.calls.at(-1)).toEqual([true]));
  });
});

describe("BrowserPanel: abas por projeto", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.browserBookmarks).mockResolvedValue([]);
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(AREA as DOMRect);
  });

  it("com o painel à mostra, avisa o projeto da conversa e lista só as abas dele", async () => {
    vi.mocked(api.browserState).mockResolvedValue({ tabs: [{ id: 1, url: "https://a.com/" }], active: 1, adblock: true });

    render(<BrowserPanel active project="/projeto-a" />);

    expect(await screen.findByText("a.com")).toBeTruthy();
    expect(api.browserSetProject).toHaveBeenCalledWith("/projeto-a");
  });

  it("trocar de projeto mostra as abas do outro e não mantém as de antes", async () => {
    vi.mocked(api.browserState)
      .mockResolvedValueOnce({ tabs: [{ id: 1, url: "https://a.com/" }], active: 1, adblock: true })
      .mockResolvedValueOnce({ tabs: [{ id: 2, url: "https://b.com/" }], active: 2, adblock: true });
    const { rerender } = render(<BrowserPanel active project="/projeto-a" />);
    await screen.findByText("a.com");

    rerender(<BrowserPanel active project="/projeto-b" />);

    expect(await screen.findByText("b.com")).toBeTruthy();
    expect(screen.queryByText("a.com")).toBeNull();
    expect(api.browserSetProject).toHaveBeenLastCalledWith("/projeto-b");
  });

  it("um painel inativo não troca o projeto que o navegador mostra", async () => {
    render(<BrowserPanel active={false} project="/projeto-a" />);

    await waitFor(() => expect(api.browserBookmarks).toHaveBeenCalled());
    expect(api.browserSetProject).not.toHaveBeenCalled();
  });
});
