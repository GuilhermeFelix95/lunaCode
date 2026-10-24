//! Skills e comandos que a CLI já conhece, para o menu de `/` do composer.
//! Só lê o frontmatter dos arquivos; o conteúdo fica com a CLI.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::claude_plugins;
use crate::logfile;
use crate::providers::ProviderKind;

/// Arquivo maior que isso não é lido: um SKILL.md de verdade cabe folgado.
const MAX_FILE_BYTES: u64 = 64_000;
const MAX_ITEMS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashItem {
    pub name: String,
    pub description: String,
    /// `skill` (pasta com SKILL.md) ou `command` (arquivo .md). Plugins do Claude: `plugin-skill` ou `plugin-command`.
    pub kind: String,
}

/// Campos `name` e `description` do bloco entre `---` no começo do arquivo.
pub fn frontmatter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let (mut name, mut description) = (None, None);
    for line in lines {
        let line = line.trim();
        if line == "---" {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim().trim_matches(['"', '\'']).to_string();
            match key.trim() {
                "name" if !value.is_empty() => name = Some(value),
                "description" if !value.is_empty() => description = Some(value),
                _ => {}
            }
        }
    }
    (name, description)
}

fn read_small(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// Uma pasta por skill: `<root>/<nome>/SKILL.md`.
fn scan_skills(root: &Path, out: &mut Vec<SlashItem>) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let dir = entry.path();
        let Some(text) = read_small(&dir.join("SKILL.md")) else { continue };
        let (name, description) = frontmatter(&text);
        let fallback = entry.file_name().to_string_lossy().into_owned();
        out.push(SlashItem {
            name: name.unwrap_or(fallback),
            description: description.unwrap_or_default(),
            kind: "skill".into(),
        });
    }
}

/// Um arquivo .md por comando; o nome é o do arquivo.
fn scan_commands(root: &Path, out: &mut Vec<SlashItem>) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        let description = read_small(&path).and_then(|text| frontmatter(&text).1);
        out.push(SlashItem {
            name: stem.to_string(),
            description: description.unwrap_or_default(),
            kind: "command".into(),
        });
    }
}

/// O que a CLI do provedor oferece: Claude lê a pasta do usuário e a do projeto; Codex, a do usuário.
pub fn collect(home: &Path, cwd: &Path, provider: ProviderKind) -> Vec<SlashItem> {
    let mut out = Vec::new();
    match provider {
        ProviderKind::Claude => {
            scan_skills(&home.join(".claude").join("skills"), &mut out);
            scan_commands(&home.join(".claude").join("commands"), &mut out);
            scan_skills(&cwd.join(".claude").join("skills"), &mut out);
            scan_commands(&cwd.join(".claude").join("commands"), &mut out);
        }
        ProviderKind::Codex => {
            scan_commands(&home.join(".codex").join("prompts"), &mut out);
            scan_skills(&home.join(".codex").join("skills"), &mut out);
        }
        // Ainda sem as pastas de comandos do OpenCode verificadas: nenhum item por enquanto.
        ProviderKind::OpenCode | ProviderKind::Cline | ProviderKind::Copilot | ProviderKind::Antigravity | ProviderKind::Hermes => {}
    }
    finish(&mut out);
    out
}

/// Ordena por nome, tira repetidos e limita a lista. Vale para as pastas da CLI e para os plugins do Claude.
fn finish(out: &mut Vec<SlashItem>) {
    out.sort_by_key(|a| a.name.to_lowercase());
    // Mesmo nome em duas pastas (ex.: skill do usuário e do projeto): fica um item só.
    out.dedup_by(|a, b| a.name == b.name);
    out.truncate(MAX_ITEMS);
}

/// Skills e comandos de um plugin do Claude, com o nome do plugin na frente (`plugin:nome`), como a CLI os chama.
pub fn plugin_items(plugin: &str, install_dir: &Path) -> Vec<SlashItem> {
    let mut out = Vec::new();
    scan_skills(&install_dir.join("skills"), &mut out);
    scan_commands(&install_dir.join("commands"), &mut out);
    out.into_iter()
        .map(|item| SlashItem {
            name: format!("{plugin}:{}", item.name),
            description: item.description,
            kind: format!("plugin-{}", item.kind),
        })
        .collect()
}

/// Itens dos plugins do Claude ligados. Se a CLI falhar, o menu segue só com as pastas locais e o erro vai para o log.
fn claude_plugin_items() -> Vec<SlashItem> {
    match claude_plugins::enabled_install_dirs() {
        Ok(dirs) => dirs
            .iter()
            .flat_map(|(id, dir)| plugin_items(id.split('@').next().unwrap_or(id.as_str()), dir))
            .collect(),
        Err(e) => {
            logfile::error(&format!("não foi possível listar os plugins do Claude para o menu de /: {e}"));
            Vec::new()
        }
    }
}

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")).map(PathBuf::from)
}

#[tauri::command]
pub async fn list_slash_items(cwd: String, provider: ProviderKind) -> Result<Vec<SlashItem>, String> {
    tauri::async_runtime::spawn_blocking(move || match home_dir() {
        Some(home) => {
            let claude = matches!(provider, ProviderKind::Claude);
            let mut items = collect(&home, Path::new(&cwd), provider);
            if claude {
                items.extend(claude_plugin_items());
                finish(&mut items);
            }
            items
        }
        None => Vec::new(),
    })
    .await
    .map_err(|e| format!("falha ao listar skills e comandos: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("lunaharness-slash-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn frontmatter_reads_name_and_description() {
        let text = "---\nname: \"revisar\"\ndescription: Revisa o diff\n---\ncorpo";

        assert_eq!(
            frontmatter(text),
            (Some("revisar".to_string()), Some("Revisa o diff".to_string()))
        );
        assert_eq!(frontmatter("sem bloco"), (None, None));
    }

    #[test]
    fn claude_lists_user_and_project_skills_and_commands() {
        let home = temp("home-claude");
        let cwd = temp("cwd-claude");
        write(&home.join(".claude/skills/pdf/SKILL.md"), "---\ndescription: Lê PDFs\n---\n");
        write(&home.join(".claude/commands/resumo.md"), "---\ndescription: Resume\n---\n");
        write(&cwd.join(".claude/skills/deploy/SKILL.md"), "---\nname: publicar\n---\n");
        write(&cwd.join(".claude/commands/teste.md"), "sem frontmatter");

        let items = collect(&home, &cwd, ProviderKind::Claude);

        let names: Vec<_> = items.iter().map(|i| (i.name.as_str(), i.kind.as_str())).collect();
        assert_eq!(
            names,
            vec![("pdf", "skill"), ("publicar", "skill"), ("resumo", "command"), ("teste", "command")]
        );
        assert_eq!(items[0].description, "Lê PDFs");
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn codex_lists_prompts_and_skills_from_the_user_folder_only() {
        let home = temp("home-codex");
        let cwd = temp("cwd-codex");
        write(&home.join(".codex/prompts/explica.md"), "---\ndescription: Explica o código\n---\n");
        write(&home.join(".codex/skills/tests/SKILL.md"), "---\nname: testes\n---\n");
        write(&cwd.join(".claude/commands/so-claude.md"), "x");

        let items = collect(&home, &cwd, ProviderKind::Codex);

        let names: Vec<_> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["explica", "testes"]);
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&cwd);
    }

    #[test]
    fn plugin_skills_and_commands_carry_the_plugin_name() {
        let install = temp("plugin-install");
        write(&install.join("skills/pdf/SKILL.md"), "---\ndescription: Lê PDFs\n---\n");
        write(&install.join("commands/resumo.md"), "---\ndescription: Resume\n---\n");

        let items = plugin_items("feature-dev", &install);

        let names: Vec<_> = items.iter().map(|i| (i.name.as_str(), i.kind.as_str())).collect();
        assert_eq!(names, vec![("feature-dev:pdf", "plugin-skill"), ("feature-dev:resumo", "plugin-command")]);
        let _ = fs::remove_dir_all(&install);
    }

    #[test]
    fn huge_or_missing_folders_do_not_break_the_list() {
        let home = temp("home-big");
        let cwd = temp("cwd-big");
        write(&home.join(".claude/commands/grande.md"), &"x".repeat(MAX_FILE_BYTES as usize + 1));

        let items = collect(&home, &cwd, ProviderKind::Claude);

        // O arquivo continua na lista (é um comando), só não é lido: a descrição fica vazia.
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].description, "");
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&cwd);
    }
}
