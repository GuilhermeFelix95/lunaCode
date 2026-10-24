//! Skills: pastas com `SKILL.md` que o Claude Code e o Codex leem.
//!
//! O marketplace oficial é embutido no app (`marketplace/skills.json`), com o conteúdo de cada arquivo
//! e o sha256 conferido antes de gravar. A instalação grava a skill nas duas CLIs:
//! `~/.claude/skills/<id>/` e `~/.codex/skills/<id>/`, de onde o menu `/` já a lista (ver slash.rs).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::plugins::marketplace::verify;
use tauri::AppHandle;

use crate::commands::app_data_dir;
use crate::slash::{frontmatter, home_dir};

pub const OFFICIAL_SKILLS: &str = include_str!("../marketplace/skills.json");
const SOURCES_FILE: &str = "skills-catalogs.json";
const MAX_FILES: usize = 20;
const MAX_FILE_BYTES: usize = 64_000;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalog {
    pub skills: Vec<SkillEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub files: Vec<SkillFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFile {
    pub path: String,
    pub sha256: String,
    pub content: String,
}

/// Skill instalada, lida do `SKILL.md` que está na pasta.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Onde está instalada: "claude" e/ou "codex".
    pub engines: Vec<String>,
}

/// Id seguro para virar nome de pasta: minúsculas, números e hífen, sem começar nem terminar com hífen.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Caminho relativo dentro da pasta da skill: sem `..`, sem barra inicial e sem letra de unidade.
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Confere a entrada: id válido, arquivos seguros, SKILL.md com nome e descrição, e hashes certos.
pub fn validate(entry: &SkillEntry) -> Result<(), String> {
    if !valid_id(&entry.id) {
        return Err(format!("id de skill inválido: {:?}", entry.id));
    }
    if entry.files.is_empty() || entry.files.len() > MAX_FILES {
        return Err(format!("a skill `{}` precisa ter de 1 a {MAX_FILES} arquivos", entry.id));
    }
    let skill_md = entry
        .files
        .iter()
        .find(|f| f.path == "SKILL.md")
        .ok_or_else(|| format!("a skill `{}` não tem SKILL.md", entry.id))?;
    for file in &entry.files {
        if !safe_path(&file.path) {
            return Err(format!("caminho de arquivo inválido na skill `{}`: {:?}", entry.id, file.path));
        }
        if file.content.len() > MAX_FILE_BYTES {
            return Err(format!("o arquivo `{}` da skill `{}` passa do limite", file.path, entry.id));
        }
        verify(&file.path, file.content.as_bytes(), &file.sha256)?;
    }
    let (name, description) = frontmatter(&skill_md.content);
    if name.is_none() || description.is_none() {
        return Err(format!("o SKILL.md da skill `{}` precisa de name e description no frontmatter", entry.id));
    }
    Ok(())
}

/// Catálogo oficial, já conferido. Erro aqui significa que o app foi montado com um arquivo errado.
pub fn official() -> Result<SkillCatalog, String> {
    let catalog: SkillCatalog =
        serde_json::from_str(OFFICIAL_SKILLS).map_err(|e| format!("catálogo de skills inválido: {e}"))?;
    for entry in &catalog.skills {
        validate(entry)?;
    }
    Ok(catalog)
}

/// Pastas de skills das CLIs dentro da casa do usuário.
fn roots(home: &Path) -> [(&'static str, PathBuf); 2] {
    [("claude", home.join(".claude").join("skills")), ("codex", home.join(".codex").join("skills"))]
}

/// Grava a skill nas duas CLIs. Primeiro escreve numa pasta temporária, depois troca pelo nome final.
pub fn install_in(home: &Path, entry: &SkillEntry) -> Result<Vec<String>, String> {
    validate(entry)?;
    let mut written = Vec::new();
    for (engine, root) in roots(home) {
        let target = root.join(&entry.id);
        if target.exists() {
            return Err(format!("a skill `{}` já está instalada em {engine}", entry.id));
        }
        fs::create_dir_all(&root).map_err(|e| format!("não foi possível criar {}: {e}", root.display()))?;
        let staging = root.join(format!(".luna-staging-{}", entry.id));
        let _ = fs::remove_dir_all(&staging);
        for file in &entry.files {
            let path = staging.join(&file.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("não foi possível criar {}: {e}", parent.display()))?;
            }
            fs::write(&path, file.content.as_bytes()).map_err(|e| format!("não foi possível gravar {}: {e}", path.display()))?;
        }
        fs::rename(&staging, &target).map_err(|e| format!("não foi possível instalar em {}: {e}", target.display()))?;
        written.push(target.display().to_string());
    }
    Ok(written)
}

/// Remove a skill das duas CLIs. Pasta que não existe não é erro.
pub fn uninstall_in(home: &Path, id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(format!("id de skill inválido: {id:?}"));
    }
    for (_, root) in roots(home) {
        let target = root.join(id);
        if target.is_dir() {
            fs::remove_dir_all(&target).map_err(|e| format!("não foi possível excluir {}: {e}", target.display()))?;
        }
    }
    Ok(())
}

/// Skills instaladas, somando as duas CLIs pelo id da pasta.
pub fn list_in(home: &Path) -> Vec<InstalledSkill> {
    let mut found: Vec<InstalledSkill> = Vec::new();
    for (engine, root) in roots(home) {
        let Ok(entries) = fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let dir = entry.path();
            let Some(id) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            let Ok(text) = fs::read_to_string(dir.join("SKILL.md")) else { continue };
            if let (Some(name), Some(description)) = frontmatter(&text) {
                match found.iter_mut().find(|s| s.id == id) {
                    Some(existing) => existing.engines.push(engine.to_string()),
                    None => found.push(InstalledSkill { id, name, description, engines: vec![engine.to_string()] }),
                }
            }
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

fn home() -> Result<PathBuf, String> {
    home_dir().ok_or_else(|| "não foi possível localizar a pasta do usuário".to_string())
}

#[tauri::command(async)]
pub fn list_skills() -> Result<Vec<InstalledSkill>, String> {
    Ok(list_in(&home()?))
}

/// Catálogo de uma URL: o mesmo formato do oficial, com o conteúdo dos arquivos dentro. Cada skill é conferida.
pub fn parse_catalog(text: &str) -> Result<SkillCatalog, String> {
    let catalog: SkillCatalog = serde_json::from_str(text).map_err(|e| format!("catálogo de skills inválido: {e}"))?;
    for entry in &catalog.skills {
        validate(entry)?;
    }
    Ok(catalog)
}

/// Catálogo oficial quando `source` é vazio; senão o catálogo da URL, que precisa estar na lista de catálogos.
async fn catalog_for(app: &AppHandle, source: Option<String>) -> Result<SkillCatalog, String> {
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
pub async fn load_skill_catalog(app: AppHandle, source: Option<String>) -> Result<SkillCatalog, String> {
    catalog_for(&app, source).await
}

#[tauri::command]
pub async fn install_skill(app: AppHandle, id: String, source: Option<String>) -> Result<Vec<String>, String> {
    let catalog = catalog_for(&app, source).await?;
    let entry = catalog
        .skills
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("skill `{id}` não está no catálogo"))?;
    install_in(&home()?, entry)
}

#[tauri::command(async)]
pub fn list_skill_sources(app: AppHandle) -> Result<Vec<String>, String> {
    Ok(crate::marketplace::load_sources_in(&app_data_dir(&app)?, SOURCES_FILE)?.urls)
}

/// Baixa e confere o catálogo antes de guardar a URL.
#[tauri::command]
pub async fn add_skill_source(app: AppHandle, url: String) -> Result<Vec<String>, String> {
    crate::marketplace::validate_https(&url)?;
    let bytes = crate::marketplace::fetch_bytes(&url).await?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{url} não é texto UTF-8"))?;
    parse_catalog(&text)?;
    Ok(crate::marketplace::add_source_in(&app_data_dir(&app)?, SOURCES_FILE, &url)?.urls)
}

#[tauri::command(async)]
pub fn remove_skill_source(app: AppHandle, url: String) -> Result<Vec<String>, String> {
    Ok(crate::marketplace::remove_source_in(&app_data_dir(&app)?, SOURCES_FILE, &url)?.urls)
}

#[tauri::command(async)]
pub fn uninstall_skill(id: String) -> Result<(), String> {
    uninstall_in(&home()?, &id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::marketplace::sha256_hex;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lunaharness-skills-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(id: &str, skill_md: &str) -> SkillEntry {
        SkillEntry {
            id: id.into(),
            name: "Teste".into(),
            description: "Skill de teste".into(),
            author: "Luna".into(),
            version: "1.0.0".into(),
            files: vec![SkillFile {
                path: "SKILL.md".into(),
                sha256: sha256_hex(skill_md.as_bytes()),
                content: skill_md.into(),
            }],
        }
    }

    const GOOD: &str = "---\nname: revisar\ndescription: Revisa mudanças\n---\nPasse o olho no diff.";

    #[test]
    fn id_so_aceita_minusculas_numeros_e_hifen() {
        assert!(valid_id("revisar-diff"));
        assert!(!valid_id("Revisar"));
        assert!(!valid_id("../x"));
        assert!(!valid_id("-x"));
    }

    #[test]
    fn recusa_arquivo_com_hash_errado_ou_sem_frontmatter() {
        let mut bad_hash = entry("revisar", GOOD);
        bad_hash.files[0].sha256 = "0".repeat(64);
        assert!(validate(&bad_hash).is_err());
        assert!(validate(&entry("revisar", "sem frontmatter")).is_err());
    }

    #[test]
    fn recusa_caminho_que_sai_da_pasta() {
        let mut e = entry("revisar", GOOD);
        e.files.push(SkillFile { path: "../fora.md".into(), sha256: sha256_hex(b"x"), content: "x".into() });
        assert!(validate(&e).is_err());
    }

    #[test]
    fn instala_nas_duas_cli_lista_e_desinstala() {
        let home = temp("install");
        install_in(&home, &entry("revisar", GOOD)).unwrap();

        assert!(home.join(".claude/skills/revisar/SKILL.md").is_file());
        assert!(home.join(".codex/skills/revisar/SKILL.md").is_file());
        let listed = list_in(&home);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].engines, vec!["claude".to_string(), "codex".to_string()]);

        assert!(install_in(&home, &entry("revisar", GOOD)).is_err());
        uninstall_in(&home, "revisar").unwrap();
        assert!(list_in(&home).is_empty());
    }

    #[test]
    fn lista_so_as_pastas_com_skill_valida() {
        let home = temp("listagem");
        let root = home.join(".claude/skills");
        fs::create_dir_all(root.join("valida")).unwrap();
        fs::write(root.join("valida/SKILL.md"), GOOD).unwrap();
        fs::create_dir_all(root.join("sem-frontmatter")).unwrap();
        fs::write(root.join("sem-frontmatter/SKILL.md"), "sem frontmatter").unwrap();
        fs::create_dir_all(root.join("vazia")).unwrap();

        let listed = list_in(&home);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "valida");
        assert_eq!(listed[0].description, "Revisa mudanças");
    }

    #[test]
    fn catalogo_oficial_passa_na_validacao() {
        assert!(official().is_ok());
    }
}
