//! Histórico de conversas em disco: `<app_data>/conversations/<id>.json`.
//! O conteúdo das mensagens é guardado como o frontend o envia; o Rust só cuida do arquivo.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::providers::{Permission, ProviderKind};

/// Resumo que a conversa guardou ao compactar. `at` = quantas mensagens existiam quando ele foi feito.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Compaction {
    pub at: usize,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    /// Nome escolhido pelo usuário na barra lateral. Com ele ligado, salvar de novo não troca o título.
    #[serde(default)]
    pub title_locked: bool,
    pub provider: ProviderKind,
    pub session_id: Option<String>,
    pub cwd: String,
    pub permission: Permission,
    /// Modelo e esforço escolhidos na conversa. Ausentes em arquivos antigos = padrão da CLI.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    /// Pasta da spec desta conversa (specs/NNN-slug). Ausente = ainda não há spec aprovada.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_dir: Option<String>,
    /// Conta extra escolhida no composer (id em accounts.json). Ausente = a conta padrão da CLI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Modo plano ligado: o agente pergunta e planeja antes de editar.
    #[serde(default)]
    pub planning: bool,
    /// Conversa iniciada no modo navegador. Só aparece nele, nunca na barra lateral do LunaCode. Ausente = LunaCode.
    #[serde(default)]
    pub browser: bool,
    /// Perfil de API usado na conversa (id em api-profiles.json). Ausente = login da CLI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Custo acumulado em dólares, quando o uso é por API. Ausente = a CLI não informou.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Último plano gerado nesta conversa (resposta do modo plano ou da spec). Ausente = nenhum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// Tarefas do agente (aba Progresso). Ficam na conversa porque o Claude Code as mantém na sessão inteira.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<Value>,
    /// Compactações da conversa: `at` é o tamanho do histórico no momento e `summary` o resumo gravado.
    /// Ausente em arquivos antigos (lista vazia).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compactions: Vec<Compaction>,
    /// Sessão nativa de cada agente e quanto do histórico ele já viu (ver src/chat/handoff.ts).
    /// Ausente em arquivos antigos: o frontend usa `session_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessions: Option<Value>,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<Value>,
}

/// Linha da barra lateral: sem as mensagens, para listar muitas conversas rápido.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub provider: ProviderKind,
    pub cwd: String,
    pub updated_at: u64,
    pub browser: bool,
}

/// Id vem do frontend (UUID minúsculo). Validar impede travessia de caminho (`../`).
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn file_for(dir: &Path, id: &str) -> Result<std::path::PathBuf, String> {
    if !is_valid_id(id) {
        return Err(format!("id de conversa inválido: `{id}`"));
    }
    Ok(dir.join(format!("{id}.json")))
}

/// Salva a conversa. Se o usuário renomeou a conversa, o título guardado vence o que o chat enviou.
pub fn save(dir: &Path, conversation: &Conversation) -> Result<(), String> {
    let mut next = conversation.clone();
    if let Ok(saved) = head(dir, &conversation.id) {
        if saved.title_locked {
            next.title = saved.title;
            next.title_locked = true;
        }
    }
    write(dir, &next)
}

const MAX_TITLE_CHARS: usize = 120;

/// Troca o nome da conversa pelo que o usuário digitou e trava o nome.
pub fn rename(dir: &Path, id: &str, title: &str) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("o nome da conversa não pode ficar vazio".into());
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("o nome da conversa passa de {MAX_TITLE_CHARS} caracteres"));
    }
    let mut conversation = load(dir, id)?;
    conversation.title = title.to_string();
    conversation.title_locked = true;
    write(dir, &conversation)
}

fn write(dir: &Path, conversation: &Conversation) -> Result<(), String> {
    let file = file_for(dir, &conversation.id)?;
    fs::create_dir_all(dir).map_err(|e| format!("não foi possível criar a pasta de conversas: {e}"))?;
    let text = serde_json::to_string(conversation)
        .map_err(|e| format!("não foi possível serializar a conversa: {e}"))?;
    // Temporário e depois troca de nome: queda no meio não deixa a conversa pela metade.
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| format!("não foi possível salvar a conversa `{}`: {e}", conversation.id))?;
    fs::rename(&tmp, &file).map_err(|e| format!("não foi possível salvar a conversa `{}`: {e}", conversation.id))
}

/// Só o cabeçalho da conversa (título, provedor, pasta e data). O serde pula as mensagens sem montá-las.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Head {
    id: String,
    title: String,
    #[serde(default)]
    title_locked: bool,
    provider: ProviderKind,
    cwd: String,
    updated_at: u64,
    #[serde(default)]
    browser: bool,
}

fn head(dir: &Path, id: &str) -> Result<Head, String> {
    let file = file_for(dir, id)?;
    let text = fs::read_to_string(&file).map_err(|e| format!("conversa `{id}` não encontrada: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("conversa `{id}` está corrompida: {e}"))
}

pub fn load(dir: &Path, id: &str) -> Result<Conversation, String> {
    let file = file_for(dir, id)?;
    let text = fs::read_to_string(&file).map_err(|e| format!("conversa `{id}` não encontrada: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("conversa `{id}` está corrompida: {e}"))
}

/// Lista as conversas, da mais recente para a mais antiga. Arquivo corrompido é ignorado
/// para não esconder as outras conversas.
pub fn list(dir: &Path) -> Vec<Summary> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut summaries: Vec<Summary> = entries
        .flatten()
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| serde_json::from_str::<Head>(&text).ok())
        .map(|c| Summary {
            id: c.id,
            title: c.title,
            provider: c.provider,
            cwd: c.cwd,
            updated_at: c.updated_at,
            browser: c.browser,
        })
        .collect();
    summaries.sort_by_key(|summary| std::cmp::Reverse(summary.updated_at));
    summaries
}

pub fn delete(dir: &Path, id: &str) -> Result<(), String> {
    let file = file_for(dir, id)?;
    fs::remove_file(&file).map_err(|e| format!("não foi possível excluir a conversa `{id}`: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lunaharness-conv-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn conversation(id: &str, updated_at: u64) -> Conversation {
        Conversation {
            id: id.into(),
            title: format!("Conversa {id}"),
            title_locked: false,
            provider: ProviderKind::Claude,
            session_id: Some("sess-1".into()),
            cwd: "C:\\projeto".into(),
            spec_dir: None,
            account: None,
            permission: Permission::AcceptEdits,
            created_at: 1,
            updated_at,
            model: None,
            effort: None,
            planning: false,
            browser: false,
            profile: None,
            cost_usd: None,
            plan: None,
            tasks: None,
            compactions: Vec::new(),
            sessions: Some(json!({"claude": {"id": "sess-1", "seen": 1}, "codex": {"id": "th-9", "seen": 3}})),
            messages: vec![json!({"role": "user", "text": "oi"})],
        }
    }

    #[test]
    fn a_renamed_conversation_keeps_its_name_when_the_open_chat_saves_again() {
        let dir = temp_dir("rename-kept");
        save(&dir, &conversation("c-1", 5)).unwrap();

        rename(&dir, "c-1", "Meu nome").unwrap();
        // O chat aberto ainda calcula o título pela primeira mensagem e salva de novo.
        save(&dir, &conversation("c-1", 6)).unwrap();

        let loaded = load(&dir, "c-1").unwrap();
        assert_eq!(loaded.title, "Meu nome");
        assert!(loaded.title_locked);
    }

    #[test]
    fn rename_refuses_an_empty_title_and_keeps_the_old_one() {
        let dir = temp_dir("rename-empty");
        save(&dir, &conversation("c-2", 5)).unwrap();

        assert!(rename(&dir, "c-2", "   ").is_err());
        assert_eq!(load(&dir, "c-2").unwrap().title, "Conversa c-2");
    }

    #[test]
    fn file_saved_before_model_and_planning_fields_still_loads() {
        let dir = temp_dir("legacy");
        fs::create_dir_all(&dir).unwrap();
        let legacy = r#"{"id":"old-1","title":"Antiga","provider":"claude","sessionId":null,
            "cwd":"C:\\p","permission":"acceptEdits","createdAt":1,"updatedAt":2,"messages":[]}"#;
        fs::write(dir.join("old-1.json"), legacy).unwrap();

        let loaded = load(&dir, "old-1").unwrap();

        assert_eq!(loaded.model, None);
        assert_eq!(loaded.effort, None);
        assert!(!loaded.planning);
        assert_eq!(loaded.sessions, None);
    }

    #[test]
    fn list_tells_browser_sessions_apart_and_old_files_count_as_luna_code() {
        let dir = temp_dir("browser-flag");
        let mut browser = conversation("nav-1", 5);
        browser.browser = true;
        save(&dir, &browser).unwrap();
        save(&dir, &conversation("luna-1", 6)).unwrap();
        fs::write(
            dir.join("old-2.json"),
            r#"{"id":"old-2","title":"Antiga","provider":"claude","sessionId":null,
            "cwd":"C:\\p","permission":"acceptEdits","createdAt":1,"updatedAt":7,"messages":[]}"#,
        )
        .unwrap();

        let listed = list(&dir);
        let flag = |id: &str| listed.iter().find(|s| s.id == id).unwrap().browser;

        assert!(flag("nav-1"));
        assert!(!flag("luna-1"));
        assert!(!flag("old-2"));
    }

    #[test]
    fn save_leaves_no_temporary_file_behind() {
        let dir = temp_dir("atomic");
        save(&dir, &conversation("atomica", 3)).unwrap();

        let names: Vec<String> = fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["atomica.json"]);
    }

    #[test]
    fn save_then_load_keeps_everything() {
        let dir = temp_dir("roundtrip");
        let original = conversation("abc-123", 10);

        save(&dir, &original).unwrap();

        assert_eq!(load(&dir, "abc-123").unwrap(), original);
    }

    #[test]
    fn profile_and_cost_survive_a_round_trip() {
        let dir = temp_dir("profile-cost");
        let mut original = conversation("perfil-1", 10);
        original.profile = Some("p1".into());
        original.cost_usd = Some(0.042);

        save(&dir, &original).unwrap();

        let loaded = load(&dir, "perfil-1").unwrap();
        assert_eq!(loaded.profile.as_deref(), Some("p1"));
        assert_eq!(loaded.cost_usd, Some(0.042));
    }

    #[test]
    fn list_orders_most_recent_first_and_skips_corrupted_files() {
        let dir = temp_dir("list");
        save(&dir, &conversation("antiga", 5)).unwrap();
        save(&dir, &conversation("recente", 50)).unwrap();
        fs::write(dir.join("quebrada.json"), "{ não é json").unwrap();

        let ids: Vec<String> = list(&dir).into_iter().map(|s| s.id).collect();

        assert_eq!(ids, ["recente", "antiga"]);
    }

    #[test]
    fn rejects_ids_that_could_escape_the_folder() {
        let dir = temp_dir("traversal");

        assert!(save(&dir, &conversation("../fora", 1)).is_err());
        assert!(load(&dir, "..\\fora").is_err());
        assert!(delete(&dir, "a/b").is_err());
    }

    #[test]
    fn delete_removes_the_conversation() {
        let dir = temp_dir("delete");
        save(&dir, &conversation("apagar", 1)).unwrap();

        delete(&dir, "apagar").unwrap();

        assert!(load(&dir, "apagar").is_err());
        assert!(list(&dir).is_empty());
    }
}
