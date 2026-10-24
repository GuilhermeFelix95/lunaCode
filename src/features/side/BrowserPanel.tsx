import { useEffect, useRef, useState, type FormEvent } from "react";
import type { ReactNode } from "react";
import { ArrowLeft, ArrowRight, Globe, History, KeyRound, Languages, Leaf, Plus, RotateCw, ShieldCheck, ShieldOff, Star, X } from "lucide-react";
import {
  browserBookmarkAdd,
  browserBookmarkRemove,
  browserBookmarks,
  browserCloseTab,
  browserHistory,
  browserNavigate,
  browserNewTab,
  browserReload,
  browserSetBounds,
  browserSetAdblock,
  browserSetEconomy,
  browserShow,
  browserTranslate,
  browserSetProject,
  browserState,
  browserSwitchTab,
  browserFillLogin,
  browserLoginSuggestions,
  browserVisits,
  onBrowserBookmarksChanged,
  onBrowserNavigated,
  onBrowserShortcut,
  onBrowserOpened,
  type BrowserBookmark,
  type BrowserBounds,
  type BrowserTab,
  type BrowserVisit,
} from "../../api";
import { buildSearchUrl, getSearchEngine, isLikelyAddress } from "../../chat/searchEngine";
import { EmptyState } from "../../ui/EmptyState";
import { t } from "../../i18n";
import { SiteIcon } from "./SiteIcon";
import { BookmarkBar } from "./BookmarkBar";

interface BrowserPanelProps {
  /** Aba do navegador à mostra. Fora dela, a página sai de cena para não cobrir o chat. */
  active: boolean;
  /** Pasta do projeto da conversa. O navegador mostra só as abas dela. */
  project?: string;
  /** Endereço pedido de fora (Ctrl+clique num link do chat): abre numa aba nova. */
  openRequest?: { url: string } | null;
  /** Controles fixos no início da faixa de abas (ex.: voltar ao LunaCode). Ausente = nada ali. */
  toolbarStart?: ReactNode;
  /** Controles fixos no fim da faixa de abas (ex.: mostrar o chat). Ausente = nada ali. */
  toolbarEnd?: ReactNode;
}

/// Limite de abas, igual ao do Rust (que é quem recusa de fato). Aqui só desabilita o botão de nova aba.
const MAX_TABS = 8;
/// Visitas mostradas numa busca do histórico: o resto fica de fora para a lista continuar leve.
const HISTORY_RESULTS = 50;
/// Teclas dos atalhos do navegador, com Ctrl: nova aba, fechar aba, barra de endereço e barra flutuante.
const SHORTCUT_KEYS = ["t", "w", "l", "f"];

/// Área onde a página aparece: a webview da aba ativa fica exatamente sobre este elemento.
function measure(area: HTMLElement): BrowserBounds {
  const rect = area.getBoundingClientRect();
  return { x: rect.left, y: rect.top, width: rect.width, height: rect.height };
}

/// Nome de uma aba: o endereço sem o esquema, ou "Nova aba" enquanto estiver em branco.
function tabLabel(tab: BrowserTab): string {
  if (!tab.url || tab.url === "about:blank") return t("browser.blankTab");
  try {
    return new URL(tab.url).hostname || tab.url;
  } catch {
    return tab.url;
  }
}

/// Verdadeiro se a aba mostra a tradução do Google (a regra fica em `translate.rs`, no Rust).
function isTranslated(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.hostname === "translate.google.com" && parsed.pathname === "/translate";
  } catch {
    return false;
  }
}

/// Nome curto de uma visita do histórico: o nome de domínio da página.
function hostnameOf(url: string): string {
  try {
    return new URL(url).hostname || url;
  } catch {
    return url;
  }
}

/// Painel que mostra a webview global agora (null = nenhum).
let browserOwner: object | null = null;

/// Navegador embutido, com várias abas. Cada aba é uma webview filha da janela, então não usa iframe e funciona com sites que bloqueiam iframes.
export function BrowserPanel({ active, project, openRequest, toolbarStart, toolbarEnd }: BrowserPanelProps) {
  const area = useRef<HTMLDivElement>(null);
  const addressInput = useRef<HTMLInputElement>(null);
  // Guarda a função de atalhos mais recente, para os ouvintes (criados uma vez) não usarem abas antigas.
  const shortcut = useRef<(key: string) => void>(() => {});
  const [spotlight, setSpotlight] = useState(false);
  const [spotlightText, setSpotlightText] = useState("");
  const [tabs, setTabs] = useState<BrowserTab[]>([]);
  const [current, setCurrent] = useState<number | null>(null);
  const [address, setAddress] = useState("");
  const [bookmarks, setBookmarks] = useState<BrowserBookmark[]>([]);
  const [error, setError] = useState<string | undefined>(undefined);
  // Histórico do app: as visitas só são pedidas quando o botão abre a lista.
  const [visits, setVisits] = useState<BrowserVisit[] | null>(null);
  const [visitQuery, setVisitQuery] = useState("");
  // Logins salvos para a página aberta (sem senha). Nada é preenchido até a pessoa escolher um.
  const [logins, setLogins] = useState<string[] | null>(null);
  // Bloqueador de anúncios: começa ligado; a pausa vale para todas as abas.
  const [adblock, setAdblock] = useState(true);

  const currentTab = tabs.find((tab) => tab.id === current) ?? null;
  const currentBookmark = currentTab ? bookmarks.find((b) => b.url === currentTab.url) : undefined;
  const translated = currentTab ? isTranslated(currentTab.url) : false;
  // Busca do histórico: o texto vale para o endereço, sem diferença de maiúsculas.
  const needle = visitQuery.trim().toLowerCase();
  const shownVisits = (visits ?? []).filter((visit) => visit.url.toLowerCase().includes(needle)).slice(0, HISTORY_RESULTS);

  // Sem projeto, lê as abas que já estão abertas (elas sobrevivem a fechar o painel). Com projeto, o efeito abaixo lê.
  useEffect(() => {
    if (project) return;
    browserState().then(
      (state) => {
        setTabs(state.tabs);
        setCurrent(state.active);
        setAddress(state.tabs.find((tab) => tab.id === state.active)?.url ?? "");
        setAdblock(state.adblock);
      },
      () => {},
    );
  }, [project]);

  // Com o painel à mostra, o navegador passa a mostrar as abas do projeto desta conversa.
  useEffect(() => {
    if (!active || !project) return;
    let stale = false;
    browserSetProject(project)
      .then(() => browserState())
      .then((state) => {
        if (stale) return;
        setTabs(state.tabs);
        setCurrent(state.active);
        setAddress(state.tabs.find((tab) => tab.id === state.active)?.url ?? "");
        setAdblock(state.adblock);
      })
      .catch(() => {});
    return () => {
      stale = true;
    };
  }, [active, project]);

  // Favoritos são do app inteiro, então leem uma vez, ao montar.
  useEffect(() => {
    browserBookmarks().then(setBookmarks, () => {});
  }, []);

  // Favoritos mudaram fora deste painel (importação, outra tela): relê a lista para a barra acompanhar.
  useEffect(() => {
    const unlisten = onBrowserBookmarksChanged(() => {
      browserBookmarks().then(setBookmarks, () => {});
    });
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, []);

  // O agente abriu uma página: o painel acompanha as abas e mostra o endereço, mesmo com a aba fechada.
  useEffect(() => {
    const unlisten = onBrowserOpened((url) => {
      browserState().then((state) => {
        setTabs(state.tabs);
        setCurrent(state.active);
        setAddress(url);
      }, () => {});
    });
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, []);

  // A página mudou sozinha (clique num link): relê as abas, para o endereço e a estrela de favorito acompanharem.
  useEffect(() => {
    const unlisten = onBrowserNavigated(() => {
      browserState().then((state) => {
        setTabs(state.tabs);
        setCurrent(state.active);
        setAddress(state.tabs.find((tab) => tab.id === state.active)?.url ?? "");
      }, () => {});
    });
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, []);

  // Link pedido de fora (Ctrl+clique no chat): cada pedido novo abre uma aba com o endereço.
  useEffect(() => {
    if (openRequest) void openTab(openRequest.url);
  }, [openRequest]);

  // Mostra a página da aba ativa quando a aba do painel está à vista e esconde quando sai.
  // A webview é uma só para o app: só o painel que a mostra pode escondê-la, senão uma conversa inativa apaga a página de outra.
  const ownerToken = useRef({}).current;
  useEffect(() => {
    if (!active) {
      if (browserOwner === null || browserOwner === ownerToken) {
        browserOwner = null;
        browserShow(false).catch(() => {});
      }
      return;
    }
    browserOwner = ownerToken;
    browserShow(true).catch(() => {});
  }, [active, current, ownerToken]);
  useEffect(() => () => {
    if (browserOwner === ownerToken) browserOwner = null;
  }, [ownerToken]);

  // Acompanha a área com a webview: muda com o tamanho do painel e da janela.
  // Um ajuste por quadro: a animação do painel dispara o ResizeObserver a cada quadro, e cada pedido ao Rust move a webview.
  useEffect(() => {
    const el = area.current;
    if (current === null || !active || !el) return;
    let frame = 0;
    function sync() {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        if (el) browserSetBounds(measure(el)).catch((e) => setError(String(e)));
      });
    }
    sync();
    const observer = new ResizeObserver(sync);
    observer.observe(el);
    window.addEventListener("resize", sync);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener("resize", sync);
    };
  }, [current, active]);

  function bounds(): BrowserBounds | undefined {
    return area.current ? measure(area.current) : undefined;
  }

  /// Abre uma aba nova: em branco, ou já com o endereço digitado.
  async function openTab(url: string | null) {
    const el = area.current;
    if (!el) return;
    try {
      const id = await browserNewTab(url, measure(el));
      setTabs((list) => [...list, { id, url: url ?? "" }]);
      setCurrent(id);
      setAddress(url ?? "");
    } catch (err) {
      setError(String(err));
    }
  }

  async function go(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    await openAddress(address);
  }

  /// Vai para um endereço ou pesquisa o texto, na aba atual; sem aba, abre uma nova com ele.
  async function openAddress(text: string) {
    const target = text.trim();
    if (!target) return;
    setError(undefined);
    // Texto que não parece endereço vira busca no motor escolhido nas configurações.
    const targetUrl = isLikelyAddress(target) ? target : buildSearchUrl(getSearchEngine(), target);
    if (current === null) {
      await openTab(targetUrl);
      return;
    }
    try {
      const url = await browserNavigate(targetUrl);
      setAddress(url);
      setTabs((list) => list.map((tab) => (tab.id === current ? { ...tab, url } : tab)));
    } catch (err) {
      setError(String(err));
    }
  }

  /// Atalho do navegador, vindo do teclado da janela ou repassado pela página. Ctrl+F não chega à busca da página.
  function runShortcut(key: string) {
    if (key === "t") {
      if (tabs.length < MAX_TABS) void openTab(null);
    } else if (key === "w") {
      if (current !== null) void closeTab(current);
    } else if (key === "l") {
      addressInput.current?.focus();
      addressInput.current?.select();
    } else if (key === "f") {
      setSpotlightText("");
      setSpotlight(true);
    }
  }
  // Os ouvintes guardam esta função pela ref, para sempre chamar a versão com as abas atuais.
  shortcut.current = runShortcut;

  // Ctrl+T/W/L/F com o foco na janela. Só o painel ativo responde.
  useEffect(() => {
    if (!active) return;
    function onKey(event: KeyboardEvent) {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      const key = event.key.toLowerCase();
      if (!SHORTCUT_KEYS.includes(key)) return;
      event.preventDefault();
      shortcut.current(key);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, shortcut]);

  // Ctrl+T/W/L/F pressionado com o foco numa página: a página repassa a tecla pelo Rust.
  useEffect(() => {
    if (!active) return;
    const unlisten = onBrowserShortcut((key) => shortcut.current(key));
    return () => {
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, [active, shortcut]);

  async function switchTo(tab: BrowserTab) {
    const box = bounds();
    if (!box) return;
    setError(undefined);
    try {
      await browserSwitchTab(tab.id, box);
      setCurrent(tab.id);
      setAddress(tab.url);
    } catch (err) {
      setError(String(err));
    }
  }

  /// Traduz a página da aba atual, ou volta ao original quando já está traduzida.
  async function toggleTranslate() {
    if (!currentTab) return;
    setError(undefined);
    try {
      const url = await browserTranslate(!isTranslated(currentTab.url));
      setAddress(url);
      setTabs((list) => list.map((tab) => (tab.id === current ? { ...tab, url } : tab)));
    } catch (err) {
      setError(String(err));
    }
  }

  /// Pausa ou religa o bloqueador de anúncios para todas as abas.
  async function toggleAdblock() {
    setError(undefined);
    const next = !adblock;
    try {
      await browserSetAdblock(next);
      setAdblock(next);
    } catch (err) {
      setError(String(err));
    }
  }

  /// Liga ou desliga a economia de memória de uma aba. A aba continua do mesmo jeito; só muda se é suspensa ao ficar escondida.
  async function toggleEconomy(tab: BrowserTab) {
    setError(undefined);
    const economize = !tab.economize;
    try {
      await browserSetEconomy(tab.id, economize);
      setTabs((list) => list.map((other) => (other.id === tab.id ? { ...other, economize } : other)));
    } catch (err) {
      setError(String(err));
    }
  }

  async function closeTab(id: number) {
    setError(undefined);
    try {
      const next = await browserCloseTab(id);
      const remaining = tabs.filter((tab) => tab.id !== id);
      setTabs(remaining);
      setCurrent(next);
      setAddress(remaining.find((tab) => tab.id === next)?.url ?? "");
    } catch (err) {
      setError(String(err));
    }
  }

  /// Favoritar guarda o endereço da aba atual; de novo, tira.
  async function toggleBookmark() {
    if (!currentTab?.url) return;
    setError(undefined);
    try {
      setBookmarks(
        currentBookmark
          ? await browserBookmarkRemove(currentBookmark.id)
          : await browserBookmarkAdd(currentTab.url, tabLabel(currentTab)),
      );
    } catch (err) {
      setError(String(err));
    }
  }

  /// Abre um favorito na aba atual, ou numa aba nova se não há nenhuma.
  async function openBookmark(bookmark: BrowserBookmark) {
    return followUrl(bookmark.url);
  }

  /// Segue um endereço: navega na aba atual, ou abre uma nova se não há nenhuma.
  async function followUrl(url: string) {
    if (current === null) return openTab(url);
    setError(undefined);
    try {
      const navigated = await browserNavigate(url);
      setAddress(navigated);
      setTabs((list) => list.map((tab) => (tab.id === current ? { ...tab, url: navigated } : tab)));
    } catch (err) {
      setError(String(err));
    }
  }

  /// Abre (ou fecha) a lista de logins salvos para a página aberta. Nada é preenchido aqui.
  async function toggleLogins() {
    setError(undefined);
    if (logins !== null) return setLogins(null);
    try {
      setLogins(await browserLoginSuggestions());
    } catch (err) {
      setError(String(err));
    }
  }

  /// Preenche o login que a pessoa escolheu. A senha sai do Rust direto para a página.
  async function fillLogin(login: string) {
    setError(undefined);
    try {
      await browserFillLogin(login);
      setLogins(null);
    } catch (err) {
      setError(String(err));
    }
  }

  /// Abre (ou fecha) a lista de visitas recentes; as visitas são lidas quando ela abre.
  async function toggleVisits() {
    setError(undefined);
    if (visits !== null) {
      setVisitQuery("");
      return setVisits(null);
    }
    try {
      setVisits(await browserVisits());
    } catch (err) {
      setError(String(err));
    }
  }

  async function removeBookmark(id: string) {
    setError(undefined);
    try {
      setBookmarks(await browserBookmarkRemove(id));
    } catch (err) {
      setError(String(err));
    }
  }

  function history(direction: "back" | "forward") {
    browserHistory(direction).catch((err) => setError(String(err)));
  }

  function reload() {
    browserReload().catch((err) => setError(String(err)));
  }

  return (
    <div className="browser">
      <div className="browser__tabbar">
        {toolbarStart}
        <div className="browser__tabs" role="tablist" aria-label={t("browser.tabsAria")}>
          {tabs.map((tab) => (
            <div key={tab.id} className={`browser__tab${tab.id === current ? " browser__tab--active" : ""}`}>
              <button
                type="button"
                role="tab"
                aria-selected={tab.id === current}
                className="browser__tab-title"
                title={tab.url || t("browser.blankTab")}
                onClick={() => void switchTo(tab)}
              >
                <SiteIcon url={tab.url} />
                {tabLabel(tab)}
              </button>
              <button
                type="button"
                className={`browser__tab-save${tab.economize ? " browser__tab-save--on" : ""}`}
                aria-pressed={Boolean(tab.economize)}
                aria-label={t("browser.economy", { title: tabLabel(tab) })}
                title={tab.economize ? t("browser.economyOnHint") : t("browser.economyOffHint")}
                onClick={() => void toggleEconomy(tab)}
              >
                <Leaf size={12} aria-hidden="true" />
              </button>
              <button
                type="button"
                className="browser__tab-close"
                aria-label={t("browser.closeTab", { title: tabLabel(tab) })}
                onClick={() => void closeTab(tab.id)}
              >
                <X size={12} aria-hidden="true" />
              </button>
            </div>
          ))}
          <button
            type="button"
            className="browser__btn"
            aria-label={t("browser.newTab")}
            title={tabs.length >= MAX_TABS ? t("browser.tabLimit", { max: MAX_TABS }) : t("browser.newTab")}
            disabled={tabs.length >= MAX_TABS}
            onClick={() => void openTab(null)}
          >
            <Plus size={14} aria-hidden="true" />
          </button>
      </div>
        {toolbarEnd}
      </div>

      <form className="browser__bar" onSubmit={go}>
        <button type="button" className="browser__btn" aria-label={t("browser.back")} title={t("browser.back")} disabled={current === null} onClick={() => history("back")}>
          <ArrowLeft size={14} aria-hidden="true" />
        </button>
        <button type="button" className="browser__btn" aria-label={t("browser.forward")} title={t("browser.forward")} disabled={current === null} onClick={() => history("forward")}>
          <ArrowRight size={14} aria-hidden="true" />
        </button>
        <button type="button" className="browser__btn" aria-label={t("browser.reload")} title={t("browser.reload")} disabled={current === null} onClick={reload}>
          <RotateCw size={14} aria-hidden="true" />
        </button>
        <input
          ref={addressInput}
          className="browser__address"
          aria-label={t("browser.addressAria")}
          placeholder={t("browser.addressPlaceholder")}
          value={address}
          spellCheck={false}
          onChange={(e) => setAddress(e.target.value)}
          onMouseDown={(e) => {
            // Clique com a barra sem foco: o navegador põe o cursor depois do foco e desfaz a seleção.
            // Cancelamos o mousedown, focamos e selecionamos aqui, uma vez só.
            if (document.activeElement === e.currentTarget) return;
            e.preventDefault();
            e.currentTarget.focus();
            e.currentTarget.select();
          }}
          onFocus={(e) => e.target.select()}
        />
        <button
          type="button"
          className={`browser__btn${currentBookmark ? " browser__btn--on" : ""}`}
          aria-pressed={Boolean(currentBookmark)}
          aria-label={currentBookmark ? t("browser.bookmarkRemove") : t("browser.bookmarkAdd")}
          title={currentBookmark ? t("browser.bookmarkRemove") : t("browser.bookmarkAdd")}
          disabled={!currentTab?.url}
          onClick={() => void toggleBookmark()}
        >
          <Star size={14} aria-hidden="true" fill={currentBookmark ? "currentColor" : "none"} />
        </button>
        <button
          type="button"
          className={`browser__btn${logins !== null ? " browser__btn--on" : ""}`}
          aria-label={t("browser.fillLogin")}
          title={t("browser.fillLogin")}
          disabled={!currentTab?.url}
          onClick={() => void toggleLogins()}
        >
          <KeyRound size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={`browser__btn${visits !== null ? " browser__btn--on" : ""}`}
          aria-label={t("browser.history")}
          title={t("browser.historyTitle")}
          onClick={() => void toggleVisits()}
        >
          <History size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={`browser__btn${translated ? " browser__btn--on" : ""}`}
          aria-pressed={translated}
          aria-label={translated ? t("browser.translateBack") : t("browser.translate")}
          title={translated ? t("browser.translateBack") : t("browser.translate")}
          disabled={!currentTab?.url}
          onClick={() => void toggleTranslate()}
        >
          <Languages size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={`browser__btn${adblock ? " browser__btn--on" : ""}`}
          aria-pressed={adblock}
          aria-label={adblock ? t("browser.adblockOn") : t("browser.adblockOff")}
          title={adblock ? t("browser.adblockOn") : t("browser.adblockOff")}
          onClick={() => void toggleAdblock()}
        >
          {adblock ? <ShieldCheck size={14} aria-hidden="true" /> : <ShieldOff size={14} aria-hidden="true" />}
        </button>
      </form>

      {visits && (
        <input
          type="search"
          className="browser__visit-search"
          aria-label={t("browser.historySearchAria")}
          placeholder={t("browser.historySearch")}
          value={visitQuery}
          onChange={(e) => setVisitQuery(e.target.value)}
        />
      )}
      {visits && (
        <ul className="browser__visits" aria-label={t("browser.visitsAria")} role="list">
          {shownVisits.length ? (
            shownVisits.map((visit) => (
              <li key={`${visit.url}:${visit.visitedAt}`} className="browser__visit">
                <button type="button" className="browser__bookmark-open" title={visit.url} onClick={() => void followUrl(visit.url)}>
                  {hostnameOf(visit.url)}
                </button>
              </li>
            ))
          ) : (
            <li className="browser__visit browser__visit--empty">{visitQuery.trim() ? t("browser.visitsNoMatch") : t("browser.visitsEmpty")}</li>
          )}
        </ul>
      )}

      {logins !== null && (
        <ul className="browser__visits" aria-label={t("browser.loginsAria")}>
          {logins.length ? (
            logins.map((login) => (
              <li key={login} className="browser__visit">
                <button type="button" className="browser__bookmark-open" onClick={() => void fillLogin(login)}>
                  {login}
                </button>
              </li>
            ))
          ) : (
            <li className="browser__visit browser__visit--empty">{t("browser.noSavedLogin")}</li>
          )}
        </ul>
      )}

      <BookmarkBar
        bookmarks={bookmarks}
        onOpen={(bookmark) => void openBookmark(bookmark)}
        onRemove={(id) => void removeBookmark(id)}
      />

      {spotlight && (
        <div className="browser-spotlight" onMouseDown={(e) => e.target === e.currentTarget && setSpotlight(false)}>
          <form
            className="browser-spotlight__form"
            role="dialog"
            aria-label={t("browser.spotlightAria")}
            onKeyDown={(e) => e.key === "Escape" && setSpotlight(false)}
            onSubmit={(e) => {
              e.preventDefault();
              setSpotlight(false);
              void openAddress(spotlightText);
            }}
          >
            <input
              autoFocus
              className="browser-spotlight__input"
              aria-label={t("browser.spotlightAria")}
              placeholder={t("browser.spotlightPlaceholder")}
              value={spotlightText}
              spellCheck={false}
              onChange={(e) => setSpotlightText(e.target.value)}
            />
          </form>
        </div>
      )}

      {error && (
        <p className="browser__error" role="alert">
          {error}
        </p>
      )}
      <div className="browser__view" ref={area}>
        {current === null && <EmptyState icon={<Globe size={18} />}>{t("browser.empty")}</EmptyState>}
      </div>
    </div>
  );
}
