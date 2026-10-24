//! Navegador integrado: uma webview filha dentro da janela principal, na aba ao lado do Terminal.
//! O agente usa a mesma webview por ferramentas MCP: o próprio executável roda em modo `browser-mcp`
//! (ver `mcp_stdio`) e fala com um servidor HTTP local, que só aceita o token gerado no início do app.
use std::cell::RefCell;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::webview::PageLoadEvent;
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Position, Rect, Size, Url, Webview, WebviewBuilder, WebviewUrl};

use crate::browser_actions::{self, page_call};
use crate::browser_import;
use crate::browser_tabs::ProjectTabs;
use crate::commands::app_data_dir;
use crate::logfile;
use crate::permission_prompt;

/// Rótulo fixo da webview do navegador.
pub const LABEL: &str = "luna-browser";
/// Evento enviado ao painel quando o agente abre uma página. O payload é o endereço.
pub const OPENED_EVENT: &str = "browser://opened";
/// Uma página terminou de carregar numa aba (inclusive depois de clicar num link): o painel relê as abas.
pub const NAVIGATED_EVENT: &str = "browser://navigated";
/// Janela principal (o Tauri dá o rótulo "main" quando o config não define outro).
const MAIN_WINDOW: &str = "main";
/// Porta do servidor de desenvolvimento do app (`devUrl` em tauri.conf.json).
const DEV_SERVER_PORT: u16 = 1420;
/// Texto da página devolvido ao agente: o começo basta, e evita estourar o contexto.
const READ_LIMIT: usize = 20_000;
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(10);
/// Quanto o agente espera a página carregar antes de ler o conteúdo.
const LOAD_TIMEOUT: Duration = Duration::from_secs(15);
/// Posição e tamanho padrão quando o agente abre o navegador antes da aba: a aba ajusta depois.
const DEFAULT_BOUNDS: Bounds = Bounds { x: 0.0, y: 0.0, width: 900.0, height: 600.0 };
const TOOL_NAMES: [&str; 9] = [
    "browser_navigate",
    "browser_read",
    "browser_snapshot",
    "browser_click",
    "browser_type",
    "browser_press_key",
    "browser_scroll",
    "browser_wait",
    "browser_search",
];
/// Tempo máximo que `browser_wait` espera um texto aparecer.
const MAX_WAIT: Duration = Duration::from_secs(15);

static APP: OnceLock<AppHandle> = OnceLock::new();
static TOKEN: OnceLock<String> = OnceLock::new();
static CONFIG: OnceLock<PathBuf> = OnceLock::new();
/// Abas abertas. Cada aba é uma webview com rótulo `LABEL-<id>`; a ativa é a que o painel mostra e o agente usa.
static TABS: Mutex<ProjectTabs> = Mutex::new(ProjectTabs::new());
/// Área onde o painel mostra as abas pela última vez: a aba que um link abre nasce nela, sem salto.
static LAST_BOUNDS: Mutex<Option<Bounds>> = Mutex::new(None);
/// Projeto de cada execução do agente: as ferramentas do navegador agem nas abas desse projeto.
static RUN_PROJECTS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

thread_local! {
    /// Projeto da chamada de ferramenta em curso nesta thread. Vazio, vale o projeto que o painel mostra.
    static CALL_PROJECT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// true desde que uma aba foi criada nesta execução do app. É o sinal de que a pessoa (ou o agente)
/// está usando o navegador, então as ferramentas dele entram nos turnos seguintes; antes disso, o MCP
/// fica desligado para não carregar 9 ferramentas no prefixo de todo turno à toa (ver `providers::browser_in_use`).
static USED: AtomicBool = AtomicBool::new(false);

/// Marca o navegador como em uso. Chamado quando uma aba nasce, seja pelo painel, pelo agente ou por um link.
pub fn mark_used() {
    USED.store(true, Ordering::Relaxed);
}

/// true quando o navegador já foi aberto nesta execução do app.
pub fn is_used() -> bool {
    USED.load(Ordering::Relaxed)
}

/// Liga a execução do agente ao projeto da conversa. Chamado quando o turno começa.
pub fn bind_run(run_id: &str, project: &str) {
    if let Ok(mut runs) = RUN_PROJECTS.lock() {
        runs.get_or_insert_with(HashMap::new).insert(run_id.to_string(), project.to_string());
    }
}

/// Desfaz `bind_run` quando o turno termina.
pub fn unbind_run(run_id: &str) {
    if let Ok(mut runs) = RUN_PROJECTS.lock() {
        if let Some(runs) = runs.as_mut() {
            runs.remove(run_id);
        }
    }
}

fn project_of_run(run_id: &str) -> Option<String> {
    RUN_PROJECTS.lock().ok()?.as_ref()?.get(run_id).cloned()
}

/// Projeto em que a chamada atua: o da execução do agente, ou o que o painel mostra.
fn target_project() -> Result<String, String> {
    if let Some(project) = CALL_PROJECT.with(|call| call.borrow().clone()) {
        return Ok(project);
    }
    TABS.lock()
        .map_err(|_| "estado do navegador travado".to_string())?
        .current()
        .map(str::to_string)
        .ok_or_else(|| "o navegador ainda não tem projeto: abra uma pasta de projeto para usar o navegador".to_string())
}

/// Marca, durante uma chamada do agente, o projeto da execução. Ao sair, a marca some.
struct CallScope;

impl CallScope {
    fn enter(project: Option<String>) -> Self {
        CALL_PROJECT.with(|call| *call.borrow_mut() = project);
        CallScope
    }
}

impl Drop for CallScope {
    fn drop(&mut self) {
        CALL_PROJECT.with(|call| *call.borrow_mut() = None);
    }
}

fn tab_label(id: u32) -> String {
    format!("{LABEL}-{id}")
}

/// Retângulo da aba em pixels lógicos, relativo à janela.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Endereço digitado ou pedido pelo agente. Só http e https; sem esquema, assume https quando parece um domínio.
pub fn parse_url(input: &str) -> Result<Url, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("endereço vazio".into());
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else if trimmed.contains('.') && !trimmed.contains(char::is_whitespace) {
        format!("https://{trimmed}")
    } else {
        return Err(format!("endereço inválido: \"{trimmed}\""));
    };
    let url = Url::parse(&candidate).map_err(|e| format!("endereço inválido: {e}"))?;
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(format!("só http e https são permitidos, não \"{other}\"")),
    }
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1"));
    if local && url.port() == Some(DEV_SERVER_PORT) {
        return Err("esse endereço é o servidor de desenvolvimento do próprio app".into());
    }
    Ok(url)
}

fn rect(bounds: Bounds) -> Result<Rect, String> {
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return Err("a área do navegador está sem tamanho".into());
    }
    Ok(Rect {
        position: Position::Logical(LogicalPosition::new(bounds.x, bounds.y)),
        size: Size::Logical(LogicalSize::new(bounds.width, bounds.height)),
    })
}

fn app() -> Result<&'static AppHandle, String> {
    APP.get().ok_or_else(|| "o app ainda não terminou de iniciar".to_string())
}

/// A aba ativa do projeto da chamada. Sem aba aberta, o agente e o painel recebem a mesma mensagem.
fn webview() -> Result<Webview, String> {
    let project = target_project()?;
    let id = TABS
        .lock()
        .map_err(|_| "estado do navegador travado".to_string())?
        .active_in(&project)
        .ok_or_else(|| "o navegador ainda não foi aberto: navegue com browser_navigate ou abra a aba Navegador".to_string())?;
    app()?
        .get_webview(&tab_label(id))
        .ok_or_else(|| "a aba ativa do navegador não existe mais".to_string())
}

/// Endereço falso usado pelo script da página para repassar um atalho. A navegação para ele é cancelada.
const SHORTCUT_SCHEME: &str = "luna-shortcut";
/// Teclas dos atalhos do navegador (com Ctrl): nova aba, fechar aba, barra de endereço e barra flutuante.
const SHORTCUT_KEYS: [&str; 4] = ["t", "w", "l", "f"];
/// Evento com a tecla de um atalho pressionado numa página do navegador.
pub const SHORTCUT_EVENT: &str = "browser://shortcut";

/// Tecla de um atalho repassado pela página, se o endereço for um dos quatro atalhos.
pub fn shortcut_key(url: &Url) -> Option<String> {
    if url.scheme() != SHORTCUT_SCHEME {
        return None;
    }
    let key = url.host_str().unwrap_or_default();
    SHORTCUT_KEYS.contains(&key).then(|| key.to_string())
}

/// Script da página: as teclas de atalho não chegam ao navegador, então a página as repassa como navegação cancelada.
fn shortcut_script() -> String {
    let keys = serde_json::to_string(&SHORTCUT_KEYS).unwrap_or_else(|_| "[]".into());
    format!(
        r#"(() => {{
  window.addEventListener('keydown', (event) => {{
    if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
    const key = event.key.toLowerCase();
    if (!{keys}.includes(key)) return;
    event.preventDefault();
    event.stopPropagation();
    location.href = "{SHORTCUT_SCHEME}://" + key;
  }}, true);
}})();"#
    )
}

/// Cria uma aba com a página e a torna a ativa. Só a ativa fica visível.
fn create_tab(url: Url, bounds: Bounds, visible: bool) -> Result<u32, String> {
    // Aba nova = navegador em uso: a partir daqui as ferramentas dele entram no turno (ver `is_used`).
    mark_used();
    let app = app()?;
    let window = app.get_window(MAIN_WINDOW).ok_or("janela principal não encontrada")?;
    let project = target_project()?;
    let id = TABS.lock().map_err(|_| "estado do navegador travado".to_string())?.reserve(&project)?;
    let page_app = app.clone();
    let shortcut_app = app.clone();
    let link_app = app.clone();
    let created = window.add_child(
        // Sem foco: a página não rouba o teclado do chat quando nasce.
        WebviewBuilder::new(&tab_label(id), WebviewUrl::External(url))
            .focused(false)
            .initialization_script(shortcut_script())
            .on_navigation(move |url| match shortcut_key(url) {
                Some(key) => {
                    let _ = shortcut_app.emit(SHORTCUT_EVENT, key);
                    false
                }
                None => true,
            })
            .on_new_window(move |url, _features| {
                // Link com target=_blank (ou window.open) vira aba do navegador. Criar a aba numa thread própria:
                // criar webview dentro do evento da página trava no Windows.
                let link_app = link_app.clone();
                std::thread::spawn(move || {
                    if let Err(e) = open_link_in_new_tab(&link_app, url) {
                        logfile::error(&format!("não foi possível abrir o link em nova aba: {e}"));
                    }
                });
                NewWindowResponse::Deny
            })
            .on_page_load(move |view, payload| {
                if payload.event() == PageLoadEvent::Finished {
                    // Histórico: cada página terminada de carregar entra nas visitas recentes.
                    if let Ok(url) = view.url() {
                        if url.scheme() == "http" || url.scheme() == "https" {
                            match app_data_dir(&page_app) {
                                Ok(dir) => {
                                    if let Err(e) = crate::browser_history::record_in(&dir, url.as_str()) {
                                        logfile::error(&format!("não foi possível registrar a visita no histórico: {e}"));
                                    }
                                }
                                Err(e) => logfile::error(&e),
                            }
                        }
                    }
                    let _ = page_app.emit(NAVIGATED_EVENT, ());
                }
            }),
        LogicalPosition::new(bounds.x, bounds.y),
        LogicalSize::new(bounds.width, bounds.height),
    );
    if let Err(e) = created {
        // A aba não chegou a existir: tira o id reservado.
        TABS.lock().map_err(|_| "estado do navegador travado".to_string())?.forget(id);
        return Err(format!("não foi possível abrir o navegador: {e}"));
    }
    if let Some(view) = app.get_webview(&tab_label(id)) {
        // Sem a senha salva, o navegador só lembra a sessão: falha aqui só vai para o log, e a aba segue.
        if let Err(e) = enable_password_autosave(&view) {
            logfile::error(&format!("não foi possível ligar o salvamento de senha da aba {id}: {e}"));
        }
        crate::adblock::install(&view);
    }
    show_only(app, id)?;
    if !visible {
        if let Some(view) = app.get_webview(&tab_label(id)) {
            view.hide().map_err(|e| e.to_string())?;
        }
    }
    Ok(id)
}

fn remember_bounds(bounds: Bounds) {
    if let Ok(mut last) = LAST_BOUNDS.lock() {
        *last = Some(bounds);
    }
}

/// Abre um link de nova janela numa aba visível, na área da última aba mostrada. Só http e https.
fn is_web_link(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

fn open_link_in_new_tab(app: &AppHandle, url: Url) -> Result<(), String> {
    if !is_web_link(&url) {
        return Err(format!("esquema de link não suportado: {}", url.scheme()));
    }
    let bounds = LAST_BOUNDS.lock().map_err(|_| "estado do navegador travado".to_string())?.unwrap_or(DEFAULT_BOUNDS);
    create_tab(url.clone(), bounds, true)?;
    // O painel relê as abas e mostra o endereço, como faz quando o agente abre uma página.
    app.emit(OPENED_EVENT, url.to_string()).map_err(|e| e.to_string())
}

/// Liga o salvamento de senha do WebView2 no perfil do app: o navegador pergunta se quer salvar e preenche depois.
/// A propriedade fica no perfil (`ICoreWebView2Profile6`), então vale para todas as abas do app.
#[cfg(windows)]
fn enable_password_autosave(view: &Webview) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2Profile6, ICoreWebView2_13};
    use windows_core::Interface;
    view.with_webview(|platform| {
        let controller = platform.controller();
        let result = unsafe {
            controller
                .CoreWebView2()
                .and_then(|core| core.cast::<ICoreWebView2_13>())
                .and_then(|core| core.Profile())
                .and_then(|profile| profile.cast::<ICoreWebView2Profile6>())
                .and_then(|profile| profile.SetIsPasswordAutosaveEnabled(true))
        };
        if let Err(e) = result {
            logfile::error(&format!("WebView2 não aceitou ligar o salvamento de senha: {e}"));
        }
    })
    .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn enable_password_autosave(_view: &Webview) -> Result<(), String> {
    Ok(())
}

/// Torna `id` a aba ativa do próprio projeto e atualiza o que aparece no painel.
fn show_only(app: &AppHandle, id: u32) -> Result<(), String> {
    let known = TABS.lock().map_err(|_| "estado do navegador travado".to_string())?.set_active(id);
    if !known {
        return Err("essa aba não está aberta".to_string());
    }
    apply_visibility(app)
}

/// Só a aba ativa do projeto atual fica visível. As outras são escondidas; as de modo economia são suspensas.
/// As demais ficam rodando, para a página não recarregar nem perder o estado ao voltar a elas.
fn apply_visibility(app: &AppHandle) -> Result<(), String> {
    let (visible, others): (Option<u32>, Vec<(u32, bool)>) = {
        let tabs = TABS.lock().map_err(|_| "estado do navegador travado".to_string())?;
        let visible = tabs.active_of_current();
        let others = tabs
            .all()
            .iter()
            .filter(|tab| Some(tab.id) != visible)
            .map(|tab| (tab.id, tab.economize))
            .collect();
        (visible, others)
    };
    for (other, economy) in others {
        if let Some(view) = app.get_webview(&tab_label(other)) {
            let _ = view.hide();
            if economy {
                set_sleeping(&view, other, true);
            }
        }
    }
    if let Some(id) = visible {
        if let Some(view) = app.get_webview(&tab_label(id)) {
            set_sleeping(&view, id, false);
            view.show().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Mostra uma aba do projeto atual e esconde as outras. Usado ao trocar de aba no painel.
fn activate_tab(id: u32) -> Result<(), String> {
    show_only(app()?, id)
}

/// Suspende (`sleeping`) ou acorda uma aba. Falha só vai para o log: a aba segue, do jeito que estava.
fn set_sleeping(view: &Webview, id: u32, sleeping: bool) {
    if let Err(e) = apply_sleep(view, sleeping) {
        let action = if sleeping { "suspender" } else { "acordar" };
        logfile::error(&format!("não foi possível {action} a aba {id} do navegador: {e}"));
    }
}

#[cfg(windows)]
fn apply_sleep(view: &Webview, sleeping: bool) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, ICoreWebView2_3, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows_core::Interface;
    view.with_webview(move |platform| {
        let controller = platform.controller();
        let result = unsafe {
            controller.CoreWebView2().and_then(|core| {
                let level = if sleeping { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
                core.cast::<ICoreWebView2_19>()?.SetMemoryUsageTargetLevel(level)?;
                let tabs = core.cast::<ICoreWebView2_3>()?;
                if sleeping {
                    tabs.TrySuspend(&webview2_com::TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(()))))
                } else {
                    tabs.Resume()
                }
            })
        };
        if let Err(e) = result {
            let action = if sleeping { "suspender" } else { "acordar" };
            logfile::error(&format!("o WebView2 recusou {action} a aba do navegador: {e}"));
        }
    })
    .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn apply_sleep(_view: &Webview, _sleeping: bool) -> Result<(), String> {
    Ok(())
}

/// Roda JavaScript na página e espera o resultado. O Tauri devolve o valor já serializado como texto JSON,
/// então um texto JSON vem com aspas em volta: desfaz essa camada.
fn eval_and_wait(view: &Webview, script: String) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    view.eval_with_callback(script, move |raw| {
        let _ = tx.send(raw);
    })
    .map_err(|e| format!("não foi possível rodar o script na página: {e}"))?;
    let raw = rx
        .recv_timeout(CALLBACK_TIMEOUT)
        .map_err(|_| "a página não respondeu a tempo".to_string())?;
    Ok(serde_json::from_str::<String>(&raw).unwrap_or(raw))
}

fn read_script() -> String {
    format!(
        "(() => JSON.stringify({{ url: location.href, title: document.title, text: (document.body ? document.body.innerText : '').slice(0, {READ_LIMIT}) }}))()"
    )
}

// ---- Comandos do painel (a aba do navegador) ----

/// Assíncrono de propósito: criar a webview num comando síncrono trava no Windows (ver `add_child`).
#[tauri::command]
pub async fn browser_new_tab(url: Option<String>, bounds: Bounds) -> Result<u32, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Sem endereço, a aba abre em branco: o usuário digita depois.
        let url = match url {
            Some(url) => parse_url(&url)?,
            None => Url::parse("about:blank").map_err(|e| e.to_string())?,
        };
        remember_bounds(bounds);
        let id = create_tab(url, bounds, true)?;
        let view = app()?.get_webview(&tab_label(id)).ok_or("a aba não existe mais")?;
        view.set_bounds(rect(bounds)?).map_err(|e| e.to_string())?;
        Ok(id)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Uma aba aberta: o id e o endereço atual da página.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    pub id: u32,
    pub url: String,
    /// Aba em modo economia de memória: suspensa quando fica escondida.
    pub economize: bool,
}

/// Abas abertas e a ativa. O painel usa para montar a barra de abas, inclusive depois de fechado e aberto.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub tabs: Vec<TabInfo>,
    pub active: Option<u32>,
    /// Bloqueador de anúncios ligado (padrão) ou pausado pelo usuário.
    pub adblock: bool,
}

/// Abas do projeto que o painel mostra, e a ativa dele. Sem projeto escolhido, a lista vem vazia.
#[tauri::command]
pub fn browser_state() -> Result<BrowserState, String> {
    let (listed, active) = {
        let tabs = TABS.lock().map_err(|_| "estado do navegador travado".to_string())?;
        let Some(project) = tabs.current() else {
            return Ok(BrowserState { tabs: Vec::new(), active: None, adblock: crate::adblock::is_enabled() });
        };
        let listed: Vec<(u32, bool)> = tabs.tabs_in(project).map(|tab| (tab.id, tab.economize)).collect();
        (listed, tabs.active_of_current())
    };
    let app = app()?;
    let tabs = listed
        .into_iter()
        .map(|(id, economize)| TabInfo {
            id,
            url: app
                .get_webview(&tab_label(id))
                .and_then(|view| view.url().ok())
                .map(|url| url.to_string())
                .unwrap_or_default(),
            economize,
        })
        .collect();
    Ok(BrowserState { tabs, active, adblock: crate::adblock::is_enabled() })
}

/// Troca o projeto que o painel mostra: as abas dele aparecem e as dos outros são escondidas.
/// Quem chama é o painel, quando a conversa aberta muda de pasta.
#[tauri::command]
pub fn browser_set_project(project: String) -> Result<(), String> {
    if project.trim().is_empty() {
        return Err("o projeto do navegador não pode ser vazio".into());
    }
    // Quem chama é o painel, quando ele abre: painel aberto = pessoa usando o navegador.
    mark_used();
    TABS.lock().map_err(|_| "estado do navegador travado".to_string())?.set_current(&project);
    apply_visibility(app()?)
}

/// Liga ou pausa o bloqueador de anúncios. Vale para todas as abas, na hora.
#[tauri::command(async)]
pub fn browser_set_adblock(on: bool) {
    crate::adblock::set_enabled(on);
}

/// Traduz a página da aba ativa, ou volta ao original quando `on` é falso. Devolve o endereço que a aba passou a mostrar.
#[tauri::command]
pub fn browser_translate(on: bool) -> Result<String, String> {
    let view = webview()?;
    let current = view.url().map_err(|e| e.to_string())?;
    let target = if on {
        crate::translate::translate_url(&current)?
    } else {
        crate::translate::original_of(&current).ok_or("esta página não está traduzida")?
    };
    view.navigate(target.clone()).map_err(|e| e.to_string())?;
    Ok(target.to_string())
}

/// Liga ou desliga a economia de memória de uma aba. Ligada, a aba é suspensa quando fica escondida.
#[tauri::command(async)]
pub fn browser_set_economy(tab_id: u32, economize: bool) -> Result<(), String> {
    let mut tabs = TABS.lock().map_err(|_| "estado do navegador travado".to_string())?;
    if !tabs.set_economy(tab_id, economize) {
        return Err("essa aba não está aberta".to_string());
    }
    Ok(())
}

/// Troca a aba ativa e posiciona a página na área do painel.
#[tauri::command]
pub fn browser_switch_tab(tab_id: u32, bounds: Bounds) -> Result<(), String> {
    activate_tab(tab_id)?;
    webview()?.set_bounds(rect(bounds)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn browser_set_bounds(bounds: Bounds) -> Result<(), String> {
    remember_bounds(bounds);
    webview()?.set_bounds(rect(bounds)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn browser_show(visible: bool) -> Result<(), String> {
    let view = match webview() {
        Ok(view) => view,
        // Sem navegador aberto, esconder não tem o que fazer.
        Err(_) if !visible => return Ok(()),
        Err(e) => return Err(e),
    };
    if visible {
        view.show().map_err(|e| e.to_string())
    } else {
        view.hide().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn browser_navigate(url: String) -> Result<String, String> {
    let url = parse_url(&url)?;
    webview()?.navigate(url.clone()).map_err(|e| e.to_string())?;
    Ok(url.to_string())
}

#[tauri::command]
pub fn browser_history(direction: String) -> Result<(), String> {
    let step = match direction.as_str() {
        "back" => "history.back()",
        "forward" => "history.forward()",
        other => return Err(format!("direção desconhecida: {other}")),
    };
    webview()?.eval(step).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn browser_reload() -> Result<(), String> {
    webview()?.reload().map_err(|e| e.to_string())
}

/// Logins guardados para a página aberta, sem senha. A origem vem da própria webview, não do front.
#[tauri::command]
pub async fn browser_login_suggestions() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let view = webview()?;
        let page = view.url().map_err(|e| e.to_string())?;
        browser_import::login_suggestions_in(&app_data_dir(app()?)?, page.as_str())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Preenche o login e a senha salvos na página aberta. Só roda quando a pessoa escolhe um login; a origem decide.
#[tauri::command]
pub async fn browser_fill_login(login: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let view = webview()?;
        let page = view.url().map_err(|e| e.to_string())?;
        let password = browser_import::password_for_login_in(&app_data_dir(app()?)?, page.as_str(), &login).map_err(|e| {
            logfile::error(&format!("preenchimento de login recusado: {e}"));
            e
        })?;
        match eval_and_wait(&view, fill_script(&login, &password)?)?.as_str() {
            "ok" => Ok(()),
            _ => Err("esta página não tem campo de senha visível".to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Script que escreve o login e a senha nos campos do formulário de login. Os valores entram como literais JSON.
fn fill_script(login: &str, password: &str) -> Result<String, String> {
    let login = serde_json::to_string(login).map_err(|e| e.to_string())?;
    let password = serde_json::to_string(password).map_err(|e| e.to_string())?;
    Ok(format!(
        r#"(() => {{
  const field = document.querySelector('input[type="password"]:not([disabled])');
  if (!field) return "sem-campo";
  const scope = field.form || document;
  const setValue = (el, value) => {{
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, value);
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }};
  const user = Array.from(scope.querySelectorAll('input:not([disabled])'))
    .filter((el) => ['text', 'email', ''].includes(el.type) && (field.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_PRECEDING))
    .pop();
  if (user) setValue(user, {login});
  setValue(field, {password});
  return "ok";
}})()"#
    ))
}

#[tauri::command]
/// Fecha uma aba. Se era a ativa, a última aberta assume. Devolve a aba ativa depois do fechamento, se houver.
pub async fn browser_close_tab(tab_id: u32) -> Result<Option<u32>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(view) = app()?.get_webview(&tab_label(tab_id)) {
            view.close().map_err(|e| e.to_string())?;
        }
        TABS.lock().map_err(|_| "estado do navegador travado".to_string())?.forget(tab_id);
        apply_visibility(app()?)?;
        Ok(TABS.lock().map_err(|_| "estado do navegador travado".to_string())?.active_of_current())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Endereço atual e título da página, como o agente os lê.
#[tauri::command]
pub async fn browser_read() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| read_page()).await.map_err(|e| e.to_string())?
}

fn read_page() -> Result<String, String> {
    let view = webview()?;
    eval_and_wait(&view, read_script())
}

// ---- Ferramentas do agente ----

/// Abre a página para o agente. Sem a aba aberta, a webview nasce escondida: a aba mostra depois.
fn tool_navigate(url: &str) -> Result<String, String> {
    let url = parse_url(url)?;
    match webview() {
        Ok(view) => view.navigate(url.clone()).map_err(|e| e.to_string())?,
        // Sem aba ativa, o agente abre uma nova, escondida: a aba do painel mostra depois.
        Err(_) => {
            create_tab(url.clone(), DEFAULT_BOUNDS, false)?;
        }
    }
    // Avisa o painel, para que o endereço apareça mesmo com a aba fechada.
    let _ = app()?.emit(OPENED_EVENT, url.as_str());
    wait_until_loaded();
    Ok(format!("Página aberta: {url}"))
}

/// Espera a página terminar de carregar, para o agente ler o conteúdo certo. Se estourar o prazo, segue assim mesmo.
fn wait_until_loaded() {
    let deadline = Instant::now() + LOAD_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(view) = webview() {
            if let Ok(state) = eval_and_wait(&view, "document.readyState".into()) {
                if state == "complete" {
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Acesso à página da aba ativa. A implementação real fala com a webview; os testes usam uma falsa.
trait BrowserPage {
    fn reply(&self, method: &str, args: &[Value]) -> Result<Value, String>;
}

/// Permissão da pessoa para ações sensíveis. A implementação real pergunta no chat da conversa.
trait ActionGate {
    fn confirm(&self, run_id: &str, action: &str, name: &str) -> Result<(), String>;
}

/// Página real: chama o script na webview da aba ativa e devolve a resposta já lida.
struct AppPage;

impl BrowserPage for AppPage {
    fn reply(&self, method: &str, args: &[Value]) -> Result<Value, String> {
        let view = webview()?;
        browser_actions::parse_page_reply(&eval_and_wait(&view, page_call(method, args))?)
    }
}

/// Permissão real: pergunta no chat da conversa que roda o turno.
struct AppGate;

impl ActionGate for AppGate {
    fn confirm(&self, run_id: &str, action: &str, name: &str) -> Result<(), String> {
        confirm(run_id, action, name)
    }
}

/// Chama um método do script da página na aba ativa e devolve a resposta já lida.
fn page_reply(method: &str, args: &[Value]) -> Result<Value, String> {
    AppPage.reply(method, args)
}

/// Lista numerada dos elementos da página, com a referência de cada um.
fn tool_snapshot() -> Result<String, String> {
    Ok(browser_actions::render_snapshot(&page_reply("snapshot", &[])?))
}

/// Referência do argumento `ref`: o número que o browser_snapshot mostra entre colchetes.
fn reference(args: &Value) -> Result<u64, String> {
    args["ref"].as_u64().filter(|r| *r >= 1).ok_or_else(|| "informe a referência (o número entre colchetes do browser_snapshot)".to_string())
}

/// Clique ou digitação numa referência. Antes de clicar, confere o nome do elemento: ação sensível pede a
/// permissão da pessoa no chat, e sem permissão o clique não acontece.
fn tool_act(run_id: &str, action: &str, args: &Value) -> Result<String, String> {
    act_with(&AppPage, &AppGate, run_id, action, args)
}

fn act_with(page: &dyn BrowserPage, gate: &dyn ActionGate, run_id: &str, action: &str, args: &Value) -> Result<String, String> {
    let reference = reference(args)?;
    let info = page.reply("inspect", &[json!(reference)])?;
    if info["ok"].as_bool() != Some(true) {
        return Err(browser_actions::page_error(&info));
    }
    let name = info["name"].as_str().unwrap_or_default();
    if action == "click" && browser_actions::is_sensitive(name) {
        gate.confirm(run_id, action, name)?;
    }
    let value = args["text"].as_str().unwrap_or_default();
    let reply = page.reply("act", &[json!(reference), json!(action), json!(value)])?;
    if reply["ok"].as_bool() == Some(true) {
        Ok(format!("ok: {action} em [{reference}] \"{name}\""))
    } else {
        Err(browser_actions::page_error(&reply))
    }
}

/// Pede a permissão da pessoa para uma ação sensível, no chat da conversa que roda o turno.
fn confirm(run_id: &str, action: &str, name: &str) -> Result<(), String> {
    if run_id.is_empty() {
        return Err("ação sensível recusada: não há conversa para pedir a permissão".into());
    }
    let input = browser_actions::confirmation_input(action, name);
    let decision = permission_prompt::ask_app(run_id, "browser_click", &input, "")?;
    if decision.allow {
        Ok(())
    } else {
        Err(decision.message.unwrap_or_else(|| "negado pela pessoa no app".into()))
    }
}

fn tool_press_key(args: &Value) -> Result<String, String> {
    let key = browser_actions::checked_key(args["key"].as_str().unwrap_or_default())?;
    let scope = args.get("ref").and_then(Value::as_u64).map_or(Value::Null, |r| json!(r));
    let reply = page_reply("key", &[json!(key), scope])?;
    if reply["ok"].as_bool() == Some(true) {
        Ok(format!("tecla enviada: {key}"))
    } else {
        Err(browser_actions::page_error(&reply))
    }
}

/// Rola a página, ou a área de rolagem de um elemento. Quantidade em pixels, limitada a 5000.
fn tool_scroll(args: &Value) -> Result<String, String> {
    let amount = args["amount"].as_i64().unwrap_or(600).clamp(0, 5000);
    let signed = match args["direction"].as_str().unwrap_or("down") {
        "down" => amount,
        "up" => -amount,
        other => return Err(format!("direção desconhecida: {other} (use up ou down)")),
    };
    let scope = args.get("ref").and_then(Value::as_u64).map_or(Value::Null, |r| json!(r));
    let reply = page_reply("scroll", &[json!(signed), scope])?;
    if reply["ok"].as_bool() == Some(true) {
        Ok(format!("rolou {signed}px em {}", reply["where"].as_str().unwrap_or("página")))
    } else {
        Err(browser_actions::page_error(&reply))
    }
}

/// Espera um texto aparecer na página, até `seconds` (no máximo `MAX_WAIT`).
fn tool_wait(args: &Value) -> Result<String, String> {
    let text = args["text"].as_str().unwrap_or_default().trim().to_string();
    if text.is_empty() {
        return Err("informe o texto que a página deve mostrar".into());
    }
    let seconds = args["seconds"].as_u64().unwrap_or(5).min(MAX_WAIT.as_secs());
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if page_reply("hasText", &[json!(text)])?.as_bool() == Some(true) {
            return Ok(format!("apareceu: \"{text}\""));
        }
        if Instant::now() >= deadline {
            return Err(format!("\"{text}\" não apareceu em {seconds} s"));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Pesquisa na web. Tenta primeiro o motor escolhido em Configurações > Navegador e troca só quando ele não
/// traz resultados. Se a página pedir login ou CAPTCHA, devolve o bloqueio em vez de resultados.
fn tool_search(args: &Value) -> Result<String, String> {
    let query = browser_actions::clean_query(args["query"].as_str().unwrap_or_default())?;
    let mut tried = Vec::new();
    for engine in browser_actions::engine_order(&browser_actions::search_engine()) {
        tool_navigate(&browser_actions::search_url(engine, &query)?)?;
        if let Some(kind) = page_reply("blocker", &[])?.as_str() {
            return Ok(browser_actions::blocker_message(kind));
        }
        let links = page_reply("links", &[])?;
        if browser_actions::has_results(&links) {
            return Ok(format!("Resultados de {engine} para \"{query}\":\n{}", browser_actions::render_links(&links)));
        }
        tried.push(engine);
    }
    Err(format!("nenhum motor de busca trouxe resultados para \"{query}\" (tentados: {})", tried.join(", ")))
}

/// Motor de busca escolhido em Configurações > Navegador. O front chama ao mudar e ao abrir o app.
#[tauri::command]
pub fn browser_set_search_engine(engine: String) -> Result<(), String> {
    browser_actions::set_search_engine(&engine)
}

/// Executa uma ferramenta pelo nome e devolve o texto para o agente. `run_id` é a conversa que pediu.
fn dispatch_tool(name: &str, args: &Value, run_id: &str) -> Result<String, String> {
    // A chamada age no projeto da conversa que a pediu, mesmo que o painel mostre outro.
    let _scope = CallScope::enter(project_of_run(run_id));
    let text = |key: &str| args[key].as_str().unwrap_or("").to_string();
    match name {
        "browser_navigate" => tool_navigate(&text("url")),
        "browser_read" => read_page(),
        "browser_snapshot" => tool_snapshot(),
        "browser_click" => tool_act(run_id, "click", args),
        "browser_type" => tool_act(run_id, "type", args),
        "browser_press_key" => tool_press_key(args),
        "browser_scroll" => tool_scroll(args),
        "browser_wait" => tool_wait(args),
        "browser_search" => tool_search(args),
        other => Err(format!("ferramenta desconhecida: {other}")),
    }
}

// ---- Servidor local (o processo MCP fala com o app por aqui) ----

pub(crate) fn new_token() -> String {
    // Chave aleatória do processo (o std a sorteia por execução), dada por duas amostras.
    let mut out = String::new();
    for _ in 0..2 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out
}

/// Sobe o servidor local e grava a configuração MCP que o Claude lê em cada turno.
pub fn start(app: AppHandle, app_data: &Path) -> Result<(), String> {
    let token = new_token();
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("não foi possível abrir a porta do navegador: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let exe = std::env::current_exe().map_err(|e| format!("não foi possível localizar o executável: {e}"))?;
    let url = format!("http://127.0.0.1:{port}/tool");
    let command = exe.display().to_string();
    let config = json!({
        "mcpServers": {
            "luna-browser": {
                "type": "stdio",
                "command": command.clone(),
                "args": ["browser-mcp"],
                "env": {
                    "LUNA_BROWSER_URL": url.clone(),
                    "LUNA_BROWSER_TOKEN": token.clone(),
                }
            }
        }
    });
    let _ = MCP_SERVER.set(McpServer { command, url, token: token.clone() });
    std::fs::create_dir_all(app_data).map_err(|e| e.to_string())?;
    let path = app_data.join("browser-mcp.json");
    std::fs::write(&path, serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?)
        .map_err(|e| format!("não foi possível gravar {}: {e}", path.display()))?;

    let _ = APP.set(app);
    let _ = TOKEN.set(token);
    let _ = CONFIG.set(path);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || handle_connection(stream));
        }
    });
    Ok(())
}

/// Caminho da configuração MCP, quando o servidor do navegador já subiu.
pub fn mcp_config() -> Option<&'static Path> {
    CONFIG.get().map(PathBuf::as_path)
}

/// Dados para subir o servidor do navegador por outro caminho que não o arquivo de configuração (Codex).
pub struct McpServer {
    pub command: String,
    pub url: String,
    pub token: String,
}

static MCP_SERVER: OnceLock<McpServer> = OnceLock::new();

/// Servidor do navegador, quando já subiu. Usado por quem recebe a configuração por argumentos.
pub fn mcp_server() -> Option<&'static McpServer> {
    MCP_SERVER.get()
}

fn handle_connection(stream: TcpStream) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    });
    let mut writer = stream;
    let response = match read_request(&mut reader) {
        Ok((path, token, body)) => answer(&path, token.as_deref(), &body),
        Err(e) => (400, json!({ "ok": false, "error": e })),
    };
    let payload = response.1.to_string();
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.0,
        if response.0 == 200 { "OK" } else { "Error" },
        payload.len()
    );
    let _ = writer.write_all(head.as_bytes());
    let _ = writer.write_all(payload.as_bytes());
}

const MAX_BODY: usize = 64 * 1024;

fn read_request(reader: &mut impl BufRead) -> Result<(String, Option<String>, String), String> {
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default().to_string();
    if method != "POST" {
        return Err("use POST".into());
    }
    let mut length = 0usize;
    let mut token = None;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).map_err(|e| e.to_string())?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().map_err(|_| "Content-Length inválido".to_string())?,
                "x-luna-token" => token = Some(value.trim().to_string()),
                _ => {}
            }
        }
    }
    if length > MAX_BODY {
        return Err("corpo grande demais".into());
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok((path, token, String::from_utf8_lossy(&body).into_owned()))
}

/// Decide a resposta de uma requisição: token, rota e corpo. Função pura para os testes.
fn answer(path: &str, token: Option<&str>, body: &str) -> (u16, Value) {
    let expected = TOKEN.get().map(String::as_str).unwrap_or("");
    if path != "/tool" {
        return (404, json!({ "ok": false, "error": "rota desconhecida" }));
    }
    if expected.is_empty() || token != Some(expected) {
        return (401, json!({ "ok": false, "error": "token inválido" }));
    }
    run_request(body)
}

fn run_request(body: &str) -> (u16, Value) {
    let request: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return (400, json!({ "ok": false, "error": format!("pedido inválido: {e}") })),
    };
    let name = request["name"].as_str().unwrap_or_default();
    let args = &request["arguments"];
    let run_id = request["runId"].as_str().unwrap_or_default();
    match dispatch_tool(name, args, run_id) {
        Ok(text) => (200, json!({ "ok": true, "text": text })),
        Err(error) => (200, json!({ "ok": false, "error": error })),
    }
}

// ---- Processo MCP (`lunacode browser-mcp`, falado pelo Claude por stdin/stdout) ----

/// Lista de ferramentas que o agente vê.
pub fn tool_specs() -> Value {
    let url_schema = json!({ "type": "object", "properties": { "url": { "type": "string", "description": "Endereço http ou https" } }, "required": ["url"] });
    let empty = json!({ "type": "object", "properties": {} });
    let reference = json!({ "type": "integer", "description": "Número entre colchetes no último browser_snapshot" });
    let key_list: Vec<&str> = browser_actions::KEYS.to_vec();
    json!([
        { "name": "browser_navigate", "description": "Abre um endereço no navegador integrado do Luna.", "inputSchema": url_schema },
        { "name": "browser_read", "description": "Lê o endereço, o título e o texto visível da página aberta.", "inputSchema": empty },
        { "name": "browser_snapshot", "description": "Lista os elementos clicáveis e os campos visíveis da página, numerados. Use antes de agir: as referências valem até a próxima leitura. Mostra o diálogo aberto (um modal) e avisa quando a página pede login, CAPTCHA ou confirmação.", "inputSchema": empty },
        { "name": "browser_click", "description": "Clica no elemento de uma referência do browser_snapshot. Ação sensível (comprar, pagar, enviar, publicar, excluir, confirmar) só acontece depois da permissão da pessoa no chat.", "inputSchema": { "type": "object", "properties": { "ref": reference }, "required": ["ref"] } },
        { "name": "browser_type", "description": "Digita um texto no campo de uma referência do browser_snapshot.", "inputSchema": { "type": "object", "properties": { "ref": reference, "text": { "type": "string" } }, "required": ["ref", "text"] } },
        { "name": "browser_press_key", "description": "Aperta uma tecla. Com ref, a tecla vai ao elemento; sem ref, ao que tem o foco. Esc fecha a maioria dos modais.", "inputSchema": { "type": "object", "properties": { "key": { "type": "string", "enum": key_list }, "ref": reference }, "required": ["key"] } },
        { "name": "browser_scroll", "description": "Rola a página, ou a área de rolagem de um elemento quando ref é dado. Use para achar botões fora da tela ou dentro de um modal.", "inputSchema": { "type": "object", "properties": { "direction": { "type": "string", "enum": ["up", "down"] }, "amount": { "type": "integer", "description": "Pixels, até 5000 (padrão 600)" }, "ref": reference }, "required": ["direction"] } },
        { "name": "browser_wait", "description": "Espera um texto aparecer na página, por até 15 segundos.", "inputSchema": { "type": "object", "properties": { "text": { "type": "string" }, "seconds": { "type": "integer" } }, "required": ["text"] } },
        { "name": "browser_search", "description": "Pesquisa na web com o motor escolhido em Configurações > Navegador, e troca de motor só quando ele não traz resultados. Abre a busca na aba ativa e devolve os links.", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] } }
    ])
}

/// Trata uma mensagem JSON-RPC. `call` executa uma ferramenta e devolve o texto ou o erro.
/// Devolve None para notificações (sem id).
pub fn handle_rpc(message: &Value, call: &mut dyn FnMut(&str, &Value) -> Result<String, String>) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message["method"].as_str().unwrap_or_default();
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "luna-browser", "version": env!("CARGO_PKG_VERSION") }
        }),
        "tools/list" => json!({ "tools": tool_specs() }),
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or_default();
            if !TOOL_NAMES.contains(&name) {
                return Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32602, "message": format!("ferramenta desconhecida: {name}") } }));
            }
            let args = &message["params"]["arguments"];
            match call(name, args) {
                Ok(text) => json!({ "content": [{ "type": "text", "text": text }], "isError": false }),
                Err(error) => json!({ "content": [{ "type": "text", "text": error }], "isError": true }),
            }
        }
        _ => return Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("método não suportado: {method}") } })),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// Entrada do modo `browser-mcp`: lê mensagens por stdin e responde por stdout.
pub fn mcp_stdio() -> i32 {
    let url = std::env::var("LUNA_BROWSER_URL").unwrap_or_default();
    let token = std::env::var("LUNA_BROWSER_TOKEN").unwrap_or_default();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("luna-browser: mensagem inválida: {e}");
                continue;
            }
        };
        let mut call = |name: &str, args: &Value| forward(&url, &token, name, args);
        if let Some(reply) = handle_rpc(&message, &mut call) {
            if writeln!(stdout, "{reply}").and_then(|_| stdout.flush()).is_err() {
                break;
            }
        }
    }
    0
}

/// Manda a ferramenta ao app pelo servidor local e lê a resposta.
fn forward(url: &str, token: &str, name: &str, args: &Value) -> Result<String, String> {
    let rest = url.strip_prefix("http://").ok_or("endereço do navegador não configurado")?;
    let (address, path) = rest.split_once('/').map(|(a, p)| (a, format!("/{p}"))).unwrap_or((rest, "/".into()));
    // A conversa que pediu a ação vai junto: a confirmação de ação sensível aparece nela.
    let run_id = std::env::var("LUNA_RUN_ID").unwrap_or_default();
    let body = json!({ "name": name, "arguments": args, "runId": run_id }).to_string();
    let mut stream = TcpStream::connect(address).map_err(|e| format!("o Luna não está respondendo: {e}"))?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nX-Luna-Token: {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = String::new();
    stream.read_to_string(&mut raw).map_err(|e| e.to_string())?;
    let payload = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or_default();
    let reply: Value = serde_json::from_str(payload).map_err(|e| format!("resposta inesperada do Luna: {e}"))?;
    if reply["ok"].as_bool() == Some(true) {
        Ok(reply["text"].as_str().unwrap_or_default().to_string())
    } else {
        Err(reply["error"].as_str().unwrap_or("falhou sem detalhe").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_http_and_https_and_guesses_https_for_domains() {
        assert_eq!(parse_url("github.com").unwrap().as_str(), "https://github.com/");
        assert_eq!(parse_url("http://localhost:3000/a").unwrap().scheme(), "http");
        assert!(parse_url("javascript://x.com/%0aalert(1)").is_err());
        assert!(parse_url("file:///C:/segredo.txt").is_err());
        assert!(parse_url("data://x.com/a").is_err());
        assert!(parse_url("   ").is_err());
        assert!(parse_url("não é endereço").is_err());
    }

    #[test]
    fn refuses_the_app_dev_server_so_the_agent_cannot_reach_the_app_itself() {
        // A devUrl do app (vite em 1420) é a própria interface: abri-la dentro do navegador daria acesso ao app.
        assert!(parse_url("http://localhost:1420/").is_err());
        assert!(parse_url("http://127.0.0.1:1420/x").is_err());
        assert!(parse_url("http://localhost:3000/").is_ok());
    }

    #[test]
    fn only_the_four_shortcut_addresses_become_keys() {
        assert_eq!(shortcut_key(&Url::parse("luna-shortcut://t").unwrap()).as_deref(), Some("t"));
        assert_eq!(shortcut_key(&Url::parse("luna-shortcut://f").unwrap()).as_deref(), Some("f"));
        // Outra tecla e outro endereço continuam sendo navegação normal.
        assert_eq!(shortcut_key(&Url::parse("luna-shortcut://x").unwrap()), None);
        assert_eq!(shortcut_key(&Url::parse("https://t.com/").unwrap()), None);
    }

    #[test]
    fn page_script_forwards_only_the_four_shortcuts() {
        let script = shortcut_script();
        assert!(script.contains("[\"t\",\"w\",\"l\",\"f\"]"));
        assert!(script.contains("location.href = \"luna-shortcut://\" + key"));
        assert!(script.contains("preventDefault()"));
    }

    #[test]
    fn fill_script_embeds_login_and_password_as_literals() {
        let script = fill_script("ana \"x\"", "senha</script>'\"").unwrap();
        // Login e senha saem como literais JSON: aspas e fechamento de script não escapam do literal.
        assert!(script.contains("setValue(user, \"ana \\\"x\\\"\")"));
        assert!(script.contains("setValue(field, \"senha</script>'\\\"\")"));
    }

    #[test]
    fn tools_are_listed_and_unknown_tools_are_refused() {
        let list = handle_rpc(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }), &mut |_, _| Ok(String::new())).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, TOOL_NAMES);

        let refused = handle_rpc(
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "shell", "arguments": {} } }),
            &mut |_, _| Ok(String::new()),
        )
        .unwrap();
        assert_eq!(refused["error"]["code"], -32602);
    }

    #[test]
    fn tool_errors_come_back_as_error_content_and_notifications_get_no_reply() {
        let failed = handle_rpc(
            &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "browser_read", "arguments": {} } }),
            &mut |_, _| Err("o navegador ainda não foi aberto".into()),
        )
        .unwrap();
        assert_eq!(failed["result"]["isError"], true);
        assert!(failed["result"]["content"][0]["text"].as_str().unwrap().contains("ainda não foi aberto"));

        assert!(handle_rpc(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), &mut |_, _| Ok(String::new())).is_none());
    }

    /// Reprodução do bug do modal (YouTube): o agente não conseguia fechar o modal porque não via os
    /// botões da página nem tinha como apertar Esc ou rolar. Sem essas ferramentas, o modal fica aberto.
    #[test]
    fn agent_can_see_page_controls_and_close_a_modal_without_a_selector() {
        for name in ["browser_snapshot", "browser_press_key", "browser_scroll"] {
            assert!(TOOL_NAMES.contains(&name), "o agente não tem a ferramenta {name}: não há como achar o botão Fechar de um modal");
        }
    }

    /// Ação sensível sem conversa para pedir permissão é recusada: o clique não acontece.
    #[test]
    fn a_sensitive_action_without_a_conversation_to_ask_is_refused() {
        let refused = confirm("", "click", "Comprar agora").unwrap_err();
        assert!(refused.contains("não há conversa"), "{refused}");
    }

    /// Página falsa: responde ao inspect com um botão sensível e registra se o clique chegou a ser enviado.
    struct FakePage {
        clicks: std::cell::Cell<u32>,
    }

    impl BrowserPage for FakePage {
        fn reply(&self, method: &str, _args: &[Value]) -> Result<Value, String> {
            match method {
                "inspect" => Ok(json!({ "ok": true, "name": "Publicar" })),
                "act" => {
                    self.clicks.set(self.clicks.get() + 1);
                    Ok(json!({ "ok": true }))
                }
                other => Err(format!("método inesperado no teste: {other}")),
            }
        }
    }

    /// Permissão falsa: a pessoa nega (ou aprova) sem abrir o chat.
    struct FakeGate {
        allow: bool,
    }

    impl ActionGate for FakeGate {
        fn confirm(&self, _run_id: &str, _action: &str, _name: &str) -> Result<(), String> {
            if self.allow { Ok(()) } else { Err("negado pela pessoa no app".into()) }
        }
    }

    /// Clique sensível sem permissão não chega à página.
    #[test]
    fn a_sensitive_click_denied_by_the_person_never_reaches_the_page() {
        let page = FakePage { clicks: std::cell::Cell::new(0) };
        let result = act_with(&page, &FakeGate { allow: false }, "run-x", "click", &json!({ "ref": 1 }));
        assert!(result.is_err());
        assert_eq!(page.clicks.get(), 0);
    }

    /// Com a permissão da pessoa, o mesmo clique sensível chega à página uma vez.
    #[test]
    fn a_sensitive_click_approved_by_the_person_reaches_the_page() {
        let page = FakePage { clicks: std::cell::Cell::new(0) };
        let result = act_with(&page, &FakeGate { allow: true }, "run-x", "click", &json!({ "ref": 1 }));
        assert!(result.is_ok());
        assert_eq!(page.clicks.get(), 1);
    }

    #[test]
    fn requests_without_the_right_token_are_refused() {
        let _ = TOKEN.set("segredo-do-teste".into());
        assert_eq!(answer("/tool", None, "{}").0, 401);
        assert_eq!(answer("/tool", Some("outro"), "{}").0, 401);
        assert_eq!(answer("/outra", Some("segredo-do-teste"), "{}").0, 404);
    }
}

#[cfg(test)]
mod tab_tests {
    use super::*;

    #[test]
    fn a_run_uses_the_project_it_was_bound_to_until_it_ends() {
        bind_run("run-teste-projeto", "/projeto-a");
        assert_eq!(project_of_run("run-teste-projeto").as_deref(), Some("/projeto-a"));

        unbind_run("run-teste-projeto");
        assert_eq!(project_of_run("run-teste-projeto"), None);
    }

    #[test]
    fn a_call_scope_is_cleared_when_it_ends() {
        {
            let _scope = CallScope::enter(Some("/projeto-b".into()));
            assert_eq!(target_project().as_deref(), Ok("/projeto-b"));
        }
        assert!(CALL_PROJECT.with(|call| call.borrow().is_none()));
    }

    #[test]
    fn tab_labels_are_unique_per_tab() {
        assert_ne!(tab_label(1), tab_label(2));
        assert!(tab_label(3).starts_with(LABEL));
    }
}

#[cfg(test)]
mod link_tests {
    use super::*;

    /// Regressão: um link com target=_blank precisa abrir numa aba nova. Só http e https entram.
    #[test]
    fn only_web_links_open_in_a_new_tab() {
        assert!(is_web_link(&Url::parse("https://x.com/home").unwrap()));
        assert!(is_web_link(&Url::parse("http://exemplo.com").unwrap()));
        assert!(!is_web_link(&Url::parse("file:///C:/segredo.txt").unwrap()));
        assert!(!is_web_link(&Url::parse("mailto:alguem@exemplo.com").unwrap()));
    }
}
