//! Servidores MCP: gravados nos arquivos de configuração das duas CLIs.
//!
//! - Claude Code: chave `mcpServers` em `~/.claude.json`. Cada variável vai como `"${NOME}"`, que o Claude expande ao conectar.
//! - Codex: tabela `[mcp_servers.<nome>]` em `~/.codex/config.toml`. As variáveis vão em `env_vars = ["NOME"]`, que o Codex repassa do ambiente.
//!
//! O app guarda só o NOME das variáveis. O valor fica no ambiente do usuário (Variáveis de ambiente do Windows),
//! e o app nunca recebe nem mostra o valor. Se um arquivo antigo tem o valor escrito, o app anota só o nome em `literal_env`.
//!
//! Cada escrita troca só a entrada do servidor pedido, sem `.bak-*`: uma cópia do arquivo guardaria os segredos de novo.
//! Servidor desligado sai dos arquivos e vai para `<app_data>/mcp-disabled.json`, para religar sem perder a config.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::AppHandle;
use toml_edit::{value, Array, DocumentMut, Item, Table};

use crate::commands::app_data_dir;
use crate::slash::home_dir;

pub const OFFICIAL_MCP: &str = include_str!("../marketplace/mcp.json");
const DISABLED_FILE: &str = "mcp-disabled.json";
const APP_ID: &str = "dev.lunacode.app";
const MAX_NAME_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub name: String,
    /// "stdio" (programa local) ou "http" (endereço remoto).
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Nomes das variáveis de ambiente que o servidor lê. O valor nunca passa pelo app.
    #[serde(default)]
    pub env_vars: Vec<String>,
    /// Variáveis que estão gravadas com o valor escrito no arquivo (formato antigo). Só o nome fica aqui.
    #[serde(default)]
    pub literal_env: Vec<String>,
    /// Onde está gravado hoje: "claude" e/ou "codex". Vazio = desligado.
    #[serde(default)]
    pub engines: Vec<String>,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

fn enabled_default() -> bool {
    true
}

/// Item do catálogo oficial. `env` lista as variáveis que a CLI deve ler do ambiente do usuário.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub transport: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub env: Vec<McpEnvField>,
}

/// `label` é a instrução que o usuário lê ("defina a variável X no Windows").
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpEnvField {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpCatalog {
    pub servers: Vec<McpCatalogEntry>,
}

/// Nome seguro para chave de TOML e de JSON: letras, números, hífen e sublinhado.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_NAME_CHARS && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Nome de variável de ambiente: letras, números e sublinhado, sem começar com número.
fn valid_env_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_CHARS
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Só https, ou http para o próprio computador.
fn valid_url(url: &str) -> bool {
    let url = url.trim();
    url.strip_prefix("https://").is_some_and(|rest| rest.len() > 3 && !rest.contains(char::is_whitespace))
        || url.starts_with("http://localhost")
        || url.starts_with("http://127.0.0.1")
}

/// Referência que o Claude Code expande: `${NOME}`.
fn reference(name: &str) -> String {
    format!("${{{name}}}")
}

pub fn validate(server: &McpServer) -> Result<(), String> {
    if !valid_name(&server.name) {
        return Err(format!("nome de servidor inválido: {:?}", server.name));
    }
    match server.transport.as_str() {
        "stdio" => {
            let command = server.command.as_deref().map(str::trim).unwrap_or("");
            if command.is_empty() {
                return Err(format!("o servidor `{}` precisa de um comando", server.name));
            }
        }
        "http" => {
            let url = server.url.as_deref().unwrap_or("");
            if !valid_url(url) {
                return Err(format!("o endereço de `{}` precisa ser https://", server.name));
            }
            if !server.env_vars.is_empty() {
                return Err(format!("o servidor remoto `{}` ainda não recebe variáveis de ambiente", server.name));
            }
        }
        other => return Err(format!("transporte desconhecido: {other:?}")),
    }
    for var in &server.env_vars {
        if !valid_env_name(var) {
            return Err(format!("nome de variável inválido: {var:?}"));
        }
    }
    Ok(())
}

/// Caminhos dos arquivos das CLIs dentro da casa do usuário.
fn claude_file(home: &Path) -> PathBuf {
    home.join(".claude.json")
}

fn codex_file(home: &Path) -> PathBuf {
    home.join(".codex").join("config.toml")
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

/// Grava o texto trocando o arquivo de forma atômica (arquivo temporário + rename). Sem backup, de propósito.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let existing = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("não foi possível ler {}: {e}", path.display())),
    };
    if existing == text {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("não foi possível criar {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("luna-tmp");
    fs::write(&tmp, text).map_err(|e| format!("não foi possível gravar {}: {e}", path.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("não foi possível salvar {}: {e}", path.display()))
}

// ---- Claude Code (~/.claude.json) ----

fn read_claude(home: &Path) -> Result<Map<String, Value>, String> {
    let path = claude_file(home);
    match fs::read_to_string(&path) {
        Ok(text) if text.trim().is_empty() => Ok(Map::new()),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => Ok(map),
            Ok(_) => Err(format!("{} não é um objeto JSON", path.display())),
            Err(e) => Err(format!("{} está corrompido: {e}", path.display())),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Map::new()),
        Err(e) => Err(format!("não foi possível ler {}: {e}", path.display())),
    }
}

/// Separa o `env` de uma entrada: `"${NOME}"` é referência; qualquer outro valor vira só o nome em `literal_env`.
/// O valor é comparado e descartado; nunca sai daqui.
fn split_claude_env(env: Option<&Map<String, Value>>) -> (Vec<String>, Vec<String>) {
    let mut refs = Vec::new();
    let mut literal = Vec::new();
    for (key, val) in env.into_iter().flatten() {
        if val.as_str() == Some(reference(key).as_str()) {
            refs.push(key.clone());
        } else {
            literal.push(key.clone());
        }
    }
    (refs, literal)
}

fn claude_entry(server: &McpServer) -> Value {
    if server.transport == "http" {
        return json!({ "type": "http", "url": server.url.clone().unwrap_or_default() });
    }
    let env: Map<String, Value> = server.env_vars.iter().map(|k| (k.clone(), Value::String(reference(k)))).collect();
    json!({
        "type": "stdio",
        "command": server.command.clone().unwrap_or_default(),
        "args": server.args,
        "env": Value::Object(env),
    })
}

fn parse_claude_entry(name: &str, entry: &Value) -> Option<McpServer> {
    let obj = entry.as_object()?;
    let mut server = McpServer { name: name.to_string(), enabled: true, engines: vec!["claude".into()], ..Default::default() };
    if let Some(url) = obj.get("url").and_then(Value::as_str) {
        server.transport = "http".into();
        server.url = Some(url.to_string());
    } else {
        server.transport = "stdio".into();
        server.command = obj.get("command").and_then(Value::as_str).map(str::to_string);
        server.args = obj
            .get("args")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let (refs, literal) = split_claude_env(obj.get("env").and_then(Value::as_object));
        server.env_vars = refs;
        server.literal_env = literal;
    }
    Some(server)
}

/// Troca (ou remove, com `None`) só a entrada `name`. Os outros servidores ficam como estão no arquivo.
fn write_claude_entry(home: &Path, name: &str, entry: Option<Value>) -> Result<(), String> {
    let mut root = read_claude(home)?;
    let mut servers = root.get("mcpServers").and_then(Value::as_object).cloned().unwrap_or_default();
    match entry {
        Some(entry) => {
            servers.insert(name.to_string(), entry);
        }
        None => {
            if servers.remove(name).is_none() {
                return Ok(());
            }
        }
    }
    if servers.is_empty() {
        root.remove("mcpServers");
    } else {
        root.insert("mcpServers".into(), Value::Object(servers));
    }
    let text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| format!("falha ao gerar o JSON: {e}"))?;
    write_atomic(&claude_file(home), &(text + "\n"))
}

// ---- Codex (~/.codex/config.toml) ----

fn read_codex(home: &Path) -> Result<DocumentMut, String> {
    let path = codex_file(home);
    match fs::read_to_string(&path) {
        Ok(text) => text.parse::<DocumentMut>().map_err(|e| format!("{} está corrompido: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(format!("não foi possível ler {}: {e}", path.display())),
    }
}

fn codex_table(server: &McpServer) -> Table {
    let mut table = Table::new();
    if server.transport == "http" {
        table["url"] = value(server.url.clone().unwrap_or_default());
    } else {
        table["command"] = value(server.command.clone().unwrap_or_default());
        let mut args = Array::new();
        for arg in &server.args {
            args.push(arg.as_str());
        }
        table["args"] = value(args);
        if !server.env_vars.is_empty() {
            let mut names = Array::new();
            for var in &server.env_vars {
                names.push(var.as_str());
            }
            table["env_vars"] = value(names);
        }
    }
    table
}

/// No Codex, a tabela `env` sempre carrega valor literal; as referências ficam em `env_vars`.
fn parse_codex_table(name: &str, table: &Table) -> McpServer {
    let text = |key: &str| table.get(key).and_then(Item::as_str).map(str::to_string);
    let mut server = McpServer { name: name.to_string(), enabled: true, engines: vec!["codex".into()], ..Default::default() };
    if let Some(url) = text("url") {
        server.transport = "http".into();
        server.url = Some(url);
    } else {
        server.transport = "stdio".into();
        server.command = text("command");
        server.args = table
            .get("args")
            .and_then(Item::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        server.env_vars = table
            .get("env_vars")
            .and_then(Item::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        server.literal_env = table
            .get("env")
            .and_then(Item::as_table_like)
            .map(|e| e.iter().map(|(k, _)| k.to_string()).collect())
            .unwrap_or_default();
    }
    server
}

/// Troca (ou remove, com `None`) só a tabela `[mcp_servers.<name>]`. O resto do arquivo, comentários incluídos, fica.
fn write_codex_entry(home: &Path, name: &str, table: Option<Table>) -> Result<(), String> {
    let mut doc = read_codex(home)?;
    match table {
        Some(table) => {
            if !doc.contains_key("mcp_servers") {
                let mut parent = Table::new();
                parent.set_implicit(true);
                doc.insert("mcp_servers", Item::Table(parent));
            }
            if let Some(parent) = doc.get_mut("mcp_servers").and_then(Item::as_table_mut) {
                parent.insert(name, Item::Table(table));
            }
        }
        None => {
            let Some(parent) = doc.get_mut("mcp_servers").and_then(Item::as_table_mut) else {
                return Ok(());
            };
            if parent.remove(name).is_none() {
                return Ok(());
            }
            if parent.is_empty() {
                doc.remove("mcp_servers");
            }
        }
    }
    write_atomic(&codex_file(home), &doc.to_string())
}

// ---- Desligados (<app_data>/mcp-disabled.json) ----

/// Formato guardado. `env` é só o formato antigo, com valores: ele vira nomes na leitura e não volta a ser gravado.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredServer {
    name: String,
    transport: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    env_vars: Vec<String>,
    #[serde(default)]
    literal_env: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

/// Lê os desligados. Se o arquivo ainda tem o formato antigo (com valores), regrava sem eles.
fn read_disabled(app_dir: &Path) -> Result<BTreeMap<String, McpServer>, String> {
    let path = app_dir.join(DISABLED_FILE);
    let stored: BTreeMap<String, StoredServer> = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{DISABLED_FILE} está corrompido: {e}"))?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("não foi possível ler {DISABLED_FILE}: {e}")),
    };
    let legacy = stored.values().any(|s| !s.env.is_empty());
    let mut servers = BTreeMap::new();
    for (key, s) in stored {
        let mut env_vars = s.env_vars;
        let mut literal_env = s.literal_env;
        for (var, val) in s.env {
            if val == reference(&var) {
                push_unique(&mut env_vars, var);
            } else {
                push_unique(&mut literal_env, var);
            }
        }
        servers.insert(
            key,
            McpServer {
                name: s.name,
                transport: s.transport,
                command: s.command,
                args: s.args,
                url: s.url,
                env_vars,
                literal_env,
                engines: Vec::new(),
                enabled: false,
            },
        );
    }
    if legacy {
        write_disabled(app_dir, &servers)?;
    }
    Ok(servers)
}

fn write_disabled(app_dir: &Path, disabled: &BTreeMap<String, McpServer>) -> Result<(), String> {
    fs::create_dir_all(app_dir).map_err(|e| format!("não foi possível criar a pasta de dados: {e}"))?;
    let text = serde_json::to_string_pretty(disabled).map_err(|e| format!("falha ao gerar {DISABLED_FILE}: {e}"))?;
    let tmp = app_dir.join(format!("{DISABLED_FILE}.tmp"));
    fs::write(&tmp, text).map_err(|e| format!("não foi possível gravar {DISABLED_FILE}: {e}"))?;
    fs::rename(&tmp, app_dir.join(DISABLED_FILE)).map_err(|e| format!("não foi possível salvar {DISABLED_FILE}: {e}"))
}

// ---- Operações ----

/// Servidores ligados nos dois arquivos, por nome. Um nome presente nas duas CLIs fica numa entrada só.
fn active_map(home: &Path) -> Result<BTreeMap<String, McpServer>, String> {
    let mut active: BTreeMap<String, McpServer> = BTreeMap::new();
    for (name, entry) in read_claude(home)?.get("mcpServers").and_then(Value::as_object).into_iter().flatten() {
        if let Some(server) = parse_claude_entry(name, entry) {
            active.insert(name.clone(), server);
        }
    }
    for (name, item) in read_codex(home)?.get("mcp_servers").and_then(Item::as_table).into_iter().flat_map(|t| t.iter()) {
        if let Some(table) = item.as_table() {
            match active.get_mut(name) {
                Some(existing) => existing.engines.push("codex".into()),
                None => {
                    active.insert(name.to_string(), parse_codex_table(name, table));
                }
            }
        }
    }
    Ok(active)
}

/// Servidores das duas CLIs somados, mais os desligados.
pub fn list_in(home: &Path, app_dir: &Path) -> Result<Vec<McpServer>, String> {
    let mut found = active_map(home)?;
    for (name, mut server) in read_disabled(app_dir)? {
        server.enabled = false;
        server.engines = Vec::new();
        found.entry(name).or_insert(server);
    }
    Ok(found.into_values().collect())
}

/// Grava (ou troca) um servidor nas duas CLIs e o tira dos desligados. Não recebe valor, só nomes de variável.
pub fn save_in(home: &Path, app_dir: &Path, server: &McpServer) -> Result<(), String> {
    validate(server)?;
    let stored = McpServer { enabled: true, engines: Vec::new(), literal_env: Vec::new(), ..server.clone() };
    write_claude_entry(home, &stored.name, Some(claude_entry(&stored)))?;
    write_codex_entry(home, &stored.name, Some(codex_table(&stored)))?;
    let mut disabled = read_disabled(app_dir)?;
    if disabled.remove(&stored.name).is_some() {
        write_disabled(app_dir, &disabled)?;
    }
    Ok(())
}

fn refuse_if_literal(server: &McpServer, action: &str) -> Result<(), String> {
    if server.literal_env.is_empty() {
        return Ok(());
    }
    Err(format!(
        "`{}` tem segredo gravado em texto ({}). Use Reconfigurar antes de {action}.",
        server.name,
        server.literal_env.join(", ")
    ))
}

/// Liga ou desliga. Desligar tira do arquivo e guarda só a configuração sem valores; ligar devolve.
/// Não desliga nem liga servidor com segredo em texto: antes é preciso Reconfigurar.
pub fn set_enabled_in(home: &Path, app_dir: &Path, name: &str, enabled: bool) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!("nome de servidor inválido: {name:?}"));
    }
    let mut disabled = read_disabled(app_dir)?;
    if enabled {
        let Some(server) = disabled.get(name).cloned() else { return Ok(()) };
        refuse_if_literal(&server, "ligar")?;
        return save_in(home, app_dir, &server);
    }
    let Some(server) = active_map(home)?.remove(name) else { return Ok(()) };
    refuse_if_literal(&server, "desligar")?;
    write_claude_entry(home, name, None)?;
    write_codex_entry(home, name, None)?;
    disabled.insert(name.to_string(), server);
    write_disabled(app_dir, &disabled)
}

/// Tira o servidor do registro global do Claude ao iniciar o Luna. Por execução, o Luna já injeta o navegador
/// escolhido quando ele está em uso (ver `providers::claude`); o registro global só gastaria tokens em toda sessão.
pub fn desligar_global_no_inicio(app_dir: &Path, name: &str) -> Result<(), String> {
    desligar_global_no_inicio_in(&home()?, app_dir, name)
}

fn desligar_global_no_inicio_in(home: &Path, app_dir: &Path, name: &str) -> Result<(), String> {
    set_enabled_in(home, app_dir, name, false)
}

/// Exclui de vez: tira dos arquivos e dos desligados.
pub fn remove_in(home: &Path, app_dir: &Path, name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!("nome de servidor inválido: {name:?}"));
    }
    write_claude_entry(home, name, None)?;
    write_codex_entry(home, name, None)?;
    let mut disabled = read_disabled(app_dir)?;
    if disabled.remove(name).is_some() {
        write_disabled(app_dir, &disabled)?;
    }
    Ok(())
}

/// Troca os valores escritos nos arquivos por referências aos nomes das variáveis.
/// O app não mostra nem guarda o valor: a entrada é removida pelo nome e gravada de novo, com `${NOME}` / `env_vars`.
/// Liga o servidor se ele estava desligado.
pub fn reconfigure_in(home: &Path, app_dir: &Path, name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!("nome de servidor inválido: {name:?}"));
    }
    let disabled = read_disabled(app_dir)?;
    let server = active_map(home)?
        .remove(name)
        .or_else(|| disabled.get(name).cloned())
        .ok_or_else(|| format!("`{name}` não está instalado"))?;
    let mut env_vars = server.env_vars.clone();
    for var in &server.literal_env {
        push_unique(&mut env_vars, var.clone());
    }
    save_in(home, app_dir, &McpServer { env_vars, literal_env: Vec::new(), enabled: true, ..server })
}

/// Confere o catálogo oficial embutido. Falha só se o binário foi gerado com um catálogo quebrado.
pub fn official() -> Result<McpCatalog, String> {
    parse_catalog(OFFICIAL_MCP).map_err(|e| format!("catálogo oficial de MCP: {e}"))
}

/// Monta o servidor a partir do catálogo. As variáveis do catálogo viram só nomes: o valor fica no ambiente do usuário.
pub fn entry_to_server(entry: &McpCatalogEntry) -> McpServer {
    McpServer {
        name: entry.id.clone(),
        transport: entry.transport.clone(),
        command: entry.command.clone(),
        args: entry.args.clone(),
        url: entry.url.clone(),
        env_vars: entry.env.iter().map(|f| f.key.clone()).collect(),
        enabled: true,
        ..Default::default()
    }
}

/// Instala um item do catálogo. Não pede valores: o usuário define as variáveis no ambiente dele.
pub fn install_from_catalog_in(home: &Path, app_dir: &Path, catalog: &McpCatalog, id: &str) -> Result<(), String> {
    let entry = catalog.servers.iter().find(|s| s.id == id).ok_or_else(|| format!("`{id}` não está no catálogo"))?;
    save_in(home, app_dir, &entry_to_server(entry))
}

/// Catálogo de uma URL: mesmo formato do oficial. Cada entrada passa pela mesma conferência.
pub fn parse_catalog(text: &str) -> Result<McpCatalog, String> {
    let catalog: McpCatalog = serde_json::from_str(text).map_err(|e| format!("catálogo de MCP inválido: {e}"))?;
    for entry in &catalog.servers {
        validate(&entry_to_server(entry))?;
    }
    Ok(catalog)
}

fn home() -> Result<PathBuf, String> {
    home_dir().ok_or_else(|| "não foi possível localizar a pasta do usuário".to_string())
}

#[tauri::command(async)]
pub fn list_mcp_servers(app: AppHandle) -> Result<Vec<McpServer>, String> {
    list_in(&home()?, &app_data_dir(&app)?)
}

#[tauri::command]
pub fn set_mcp_enabled(app: AppHandle, name: String, enabled: bool) -> Result<(), String> {
    set_enabled_in(&home()?, &app_data_dir(&app)?, &name, enabled)
}

#[tauri::command(async)]
pub fn remove_mcp_server(app: AppHandle, name: String) -> Result<(), String> {
    remove_in(&home()?, &app_data_dir(&app)?, &name)
}

/// Troca o segredo em texto por referência. Não recebe nem devolve valores.
#[tauri::command(async)]
pub fn reconfigure_mcp_server(app: AppHandle, name: String) -> Result<(), String> {
    reconfigure_in(&home()?, &app_data_dir(&app)?, &name)
}

const SOURCES_FILE: &str = "mcp-catalogs.json";

/// Catálogo oficial quando `source` é vazio; senão o catálogo da URL, que precisa estar na lista de catálogos.
async fn catalog_for(app: &AppHandle, source: Option<String>) -> Result<McpCatalog, String> {
    match source.filter(|s| !s.is_empty()) {
        None => official(),
        Some(url) => {
            let sources = crate::marketplace::load_sources_in(&app_data_dir(app)?, SOURCES_FILE)?;
            if !sources.urls.contains(&url) {
                return Err(format!("o catálogo {url} não está na sua lista"));
            }
            let bytes = crate::marketplace::fetch_bytes(&url).await?;
            let text = String::from_utf8(bytes).map_err(|_| format!("{url} não é texto UTF-8"))?;
            parse_catalog(&text)
        }
    }
}

#[tauri::command]
pub async fn load_mcp_catalog(app: AppHandle, source: Option<String>) -> Result<McpCatalog, String> {
    catalog_for(&app, source).await
}

#[tauri::command]
pub async fn install_mcp_from_catalog(app: AppHandle, id: String, source: Option<String>) -> Result<(), String> {
    let catalog = catalog_for(&app, source).await?;
    install_from_catalog_in(&home()?, &app_data_dir(&app)?, &catalog, &id)
}

#[tauri::command(async)]
pub fn list_mcp_sources(app: AppHandle) -> Result<Vec<String>, String> {
    Ok(crate::marketplace::load_sources_in(&app_data_dir(&app)?, SOURCES_FILE)?.urls)
}

/// Baixa e confere o catálogo antes de guardar a URL.
#[tauri::command]
pub async fn add_mcp_source(app: AppHandle, url: String) -> Result<Vec<String>, String> {
    crate::marketplace::validate_https(&url)?;
    let bytes = crate::marketplace::fetch_bytes(&url).await?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{url} não é texto UTF-8"))?;
    parse_catalog(&text)?;
    Ok(crate::marketplace::add_source_in(&app_data_dir(&app)?, SOURCES_FILE, &url)?.urls)
}

#[tauri::command(async)]
pub fn remove_mcp_source(app: AppHandle, url: String) -> Result<Vec<String>, String> {
    Ok(crate::marketplace::remove_source_in(&app_data_dir(&app)?, SOURCES_FILE, &url)?.urls)
}

/// Grava um servidor escrito à mão. Só os nomes das variáveis entram; o valor fica no ambiente do usuário.
#[tauri::command]
pub fn save_mcp_server(app: AppHandle, server: McpServer) -> Result<(), String> {
    save_in(&home()?, &app_data_dir(&app)?, &server)
}

// ---- Terminal: `lunacode mcp add <id>` e `lunacode mcp remove <nome>` ----

/// Pasta de dados do app, igual à que o Tauri usa com o identificador `dev.lunacode.app`.
fn app_dir_for_cli() -> Result<PathBuf, String> {
    if cfg!(windows) {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().unwrap_or_default().join("AppData").join("Roaming"));
        Ok(base.join(APP_ID))
    } else if cfg!(target_os = "macos") {
        Ok(home()?.join("Library").join("Application Support").join(APP_ID))
    } else {
        let base = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().unwrap_or_default().join(".local").join("share"));
        Ok(base.join(APP_ID))
    }
}

fn confirm(question: &str) -> Result<bool, String> {
    print!("{question}");
    io::stdout().flush().map_err(|e| format!("não foi possível mostrar a pergunta: {e}"))?;
    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer).map_err(|e| format!("não foi possível ler a resposta: {e}"))?;
    Ok(answer.trim().eq_ignore_ascii_case("s"))
}

/// Ponto de entrada do modo terminal. Devolve o código de saída.
pub fn cli(args: &[String]) -> i32 {
    match run_cli(args) {
        Ok(message) => {
            println!("{message}");
            0
        }
        Err(error) => {
            eprintln!("lunacode: {error}");
            1
        }
    }
}

fn run_cli(args: &[String]) -> Result<String, String> {
    let home = home()?;
    let app_dir = app_dir_for_cli()?;
    match args {
        [action, id] if action == "add" => {
            let catalog = official()?;
            let entry = catalog.servers.iter().find(|s| &s.id == id).ok_or_else(|| format!("`{id}` não está no catálogo"))?;
            let server = entry_to_server(entry);
            let launch = if server.transport == "http" {
                server.url.clone().unwrap_or_default()
            } else {
                format!("{} {}", server.command.clone().unwrap_or_default(), server.args.join(" "))
            };
            println!("Servidor \"{}\" será gravado nas duas CLIs (Claude Code e Codex).", entry.name);
            println!("  Executa: {launch}");
            if !server.env_vars.is_empty() {
                println!("  Lê do seu ambiente: {}. Defina essas variáveis; o valor não passa pelo LunaCode.", server.env_vars.join(", "));
            }
            if !confirm("Instalar? (s/N) ")? {
                return Ok("cancelado. Nada foi alterado.".into());
            }
            install_from_catalog_in(&home, &app_dir, &catalog, id)?;
            Ok(format!("\"{}\" instalado. Reinicie as CLIs para ele aparecer.", entry.name))
        }
        [action, name] if action == "remove" => {
            if !confirm(&format!("Remover \"{name}\" das duas CLIs? (s/N) "))? {
                return Ok("cancelado. Nada foi alterado.".into());
            }
            remove_in(&home, &app_dir, name)?;
            Ok(format!("\"{name}\" removido."))
        }
        _ => Err("uso: lunacode mcp add <id> | lunacode mcp remove <nome>".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lunaharness-mcp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stdio(name: &str) -> McpServer {
        McpServer {
            name: name.into(),
            transport: "stdio".into(),
            command: Some("npx".into()),
            args: vec!["-y".into(), "pacote".into()],
            env_vars: vec!["TOKEN_X".into()],
            ..Default::default()
        }
    }

    /// Arquivo do Claude com um servidor que ainda tem o valor escrito (formato antigo).
    fn claude_with_literal(home: &Path) {
        fs::write(
            home.join(".claude.json"),
            r#"{"mcpServers": {"memoria": {"type": "stdio", "command": "npx", "args": [], "env": {"TOKEN_X": "segredo-123"}}}}"#,
        )
        .unwrap();
    }

    #[test]
    fn nome_e_url_seguros() {
        assert!(valid_name("memoria_1"));
        assert!(!valid_name("../x"));
        assert!(valid_url("https://mcp.exemplo.com/mcp"));
        assert!(!valid_url("http://exemplo.com"));
    }

    #[test]
    fn grava_so_referencias_nas_duas_cli_preservando_o_resto() {
        let home = temp("save");
        fs::write(home.join(".claude.json"), r#"{"numStartups": 7, "projects": {"a": {}}}"#).unwrap();
        save_in(&home, &temp("save-app"), &stdio("memoria")).unwrap();

        let claude: Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
        assert_eq!(claude["numStartups"], 7);
        assert_eq!(claude["mcpServers"]["memoria"]["env"]["TOKEN_X"], "${TOKEN_X}");
        let codex = fs::read_to_string(home.join(".codex/config.toml")).unwrap();
        assert!(codex.contains("[mcp_servers.memoria]"));
        assert_eq!(list_in(&home, &temp("save-app-2")).unwrap()[0].env_vars, vec!["TOKEN_X".to_string()]);
    }

    #[test]
    fn nenhum_arquivo_recebe_backup() {
        let home = temp("no-backup");
        fs::write(home.join(".claude.json"), "{}").unwrap();
        save_in(&home, &temp("no-backup-app"), &stdio("memoria")).unwrap();

        let names: Vec<String> = fs::read_dir(&home).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| !n.contains(".bak-")), "{names:?}");
    }

    #[test]
    fn config_do_codex_mantem_comentarios() {
        let home = temp("comments");
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/config.toml"), "# meu modelo favorito\nmodel = \"x\"\n").unwrap();

        save_in(&home, &temp("comments-app"), &stdio("memoria")).unwrap();

        let codex = fs::read_to_string(home.join(".codex/config.toml")).unwrap();
        assert!(codex.contains("# meu modelo favorito"));
        assert!(codex.contains("model = \"x\""));
    }

    #[test]
    fn desligar_tira_dos_arquivos_e_religar_devolve() {
        let home = temp("toggle");
        let app = temp("toggle-app");
        save_in(&home, &app, &stdio("memoria")).unwrap();

        set_enabled_in(&home, &app, "memoria", false).unwrap();
        let listed = list_in(&home, &app).unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].enabled);
        let claude: Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap_or(Value::Null);
        assert!(claude.get("mcpServers").is_none());

        set_enabled_in(&home, &app, "memoria", true).unwrap();
        let listed = list_in(&home, &app).unwrap();
        assert!(listed[0].enabled);
        assert_eq!(listed[0].command.as_deref(), Some("npx"));
        assert_eq!(listed[0].env_vars, vec!["TOKEN_X".to_string()]);
        assert_eq!(listed[0].engines.len(), 2);
    }

    #[test]
    fn recusa_nome_ruim_e_env_em_servidor_remoto() {
        let home = temp("bad");
        let app = temp("bad-app");
        assert!(save_in(&home, &app, &stdio("../fora")).is_err());
        let remote = McpServer {
            name: "remoto".into(),
            transport: "http".into(),
            url: Some("https://mcp.exemplo.com/mcp".into()),
            env_vars: vec!["TOKEN_X".into()],
            ..Default::default()
        };
        assert!(validate(&remote).is_err());
    }

    #[test]
    fn instalar_do_catalogo_grava_so_o_nome_da_variavel() {
        let home = temp("catalog");
        let app = temp("catalog-app");
        install_from_catalog_in(&home, &app, &official().unwrap(), "github").unwrap();

        let claude = fs::read_to_string(home.join(".claude.json")).unwrap();
        assert!(claude.contains("${GITHUB_PERSONAL_ACCESS_TOKEN}"));
        let github = list_in(&home, &app).unwrap().into_iter().find(|s| s.name == "github").unwrap();
        assert_eq!(github.env_vars, vec!["GITHUB_PERSONAL_ACCESS_TOKEN".to_string()]);
        assert!(github.literal_env.is_empty());
    }

    #[test]
    fn valor_antigo_aparece_so_pelo_nome_e_nao_vai_para_a_lista() {
        let home = temp("literal");
        claude_with_literal(&home);

        let listed = list_in(&home, &temp("literal-app")).unwrap();

        assert_eq!(listed[0].literal_env, vec!["TOKEN_X".to_string()]);
        assert!(!serde_json::to_string(&listed).unwrap().contains("segredo-123"));
    }

    #[test]
    fn valor_antigo_do_codex_aparece_so_pelo_nome() {
        let home = temp("literal-codex");
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::write(home.join(".codex/config.toml"), "[mcp_servers.memoria]\ncommand = \"npx\"\n\n[mcp_servers.memoria.env]\nTOKEN_X = \"segredo-456\"\n").unwrap();

        let listed = list_in(&home, &temp("literal-codex-app")).unwrap();

        assert_eq!(listed[0].literal_env, vec!["TOKEN_X".to_string()]);
        assert!(!serde_json::to_string(&listed).unwrap().contains("segredo-456"));
    }

    #[test]
    fn reconfigurar_troca_o_valor_por_referencia() {
        let home = temp("reconfigure");
        let app = temp("reconfigure-app");
        claude_with_literal(&home);

        reconfigure_in(&home, &app, "memoria").unwrap();

        let text = fs::read_to_string(home.join(".claude.json")).unwrap();
        assert!(!text.contains("segredo-123"));
        assert!(text.contains("${TOKEN_X}"));
        let listed = list_in(&home, &app).unwrap();
        assert!(listed[0].literal_env.is_empty());
        assert_eq!(listed[0].env_vars, vec!["TOKEN_X".to_string()]);
    }

    #[test]
    fn desligar_recusa_segredo_em_texto_e_nao_mexe_no_arquivo() {
        let home = temp("refuse");
        let app = temp("refuse-app");
        claude_with_literal(&home);
        let before = fs::read_to_string(home.join(".claude.json")).unwrap();

        let err = set_enabled_in(&home, &app, "memoria", false).unwrap_err();

        assert!(err.contains("Reconfigurar"));
        assert!(err.contains("TOKEN_X"));
        assert!(!err.contains("segredo-123"));
        assert_eq!(fs::read_to_string(home.join(".claude.json")).unwrap(), before);
    }

    #[test]
    fn salvar_um_servidor_nao_mexe_nos_outros() {
        let home = temp("others");
        fs::write(
            home.join(".claude.json"),
            r#"{"mcpServers": {"outro": {"type": "stdio", "command": "x", "args": [], "env": {"K": "intocado"}}}}"#,
        )
        .unwrap();

        save_in(&home, &temp("others-app"), &stdio("memoria")).unwrap();

        let text = fs::read_to_string(home.join(".claude.json")).unwrap();
        assert!(text.contains("\"intocado\""));
        assert!(text.contains("\"memoria\""));
    }

    #[test]
    fn formato_antigo_do_desligado_e_convertido_e_regravado_sem_valor() {
        let home = temp("legacy-disabled");
        let app = temp("legacy-disabled-app");
        fs::write(
            app.join(DISABLED_FILE),
            r#"{"memoria": {"name": "memoria", "transport": "stdio", "command": "npx", "args": [], "env": {"TOKEN_X": "segredo-789"}, "enabled": false}}"#,
        )
        .unwrap();

        let listed = list_in(&home, &app).unwrap();

        assert_eq!(listed[0].literal_env, vec!["TOKEN_X".to_string()]);
        let text = fs::read_to_string(app.join(DISABLED_FILE)).unwrap();
        assert!(!text.contains("segredo-789"));
    }

    #[test]
    fn remove_de_vez() {
        let home = temp("remove");
        let app = temp("remove-app");
        save_in(&home, &app, &stdio("memoria")).unwrap();
        set_enabled_in(&home, &app, "memoria", false).unwrap();

        remove_in(&home, &app, "memoria").unwrap();

        assert!(list_in(&home, &app).unwrap().is_empty());
    }

    #[test]
    fn catalogo_oficial_passa_na_validacao() {
        assert!(official().is_ok());
    }

    #[test]
    fn servidor_ativo_some_do_claude_ao_ser_mantido_desligado() {
        let home = temp("keep-off");
        let app = temp("keep-off-app");
        save_in(&home, &app, &stdio("playwright")).unwrap();

        desligar_global_no_inicio_in(&home, &app, "playwright").unwrap();

        let claude = read_claude(&home).unwrap();
        assert!(claude
            .get("mcpServers")
            .and_then(|servers| servers.get("playwright"))
            .is_none());
        assert!(read_disabled(&app).unwrap().contains_key("playwright"));
    }

    #[test]
    fn manter_desligado_sem_servidor_registrado_nao_cria_nada() {
        let home = temp("keep-off-empty");
        let app = temp("keep-off-empty-app");

        desligar_global_no_inicio_in(&home, &app, "playwright").unwrap();

        assert!(!home.join(".claude.json").exists());
        assert!(read_disabled(&app).unwrap().is_empty());
    }
}
