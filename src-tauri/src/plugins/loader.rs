use std::fs;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

/// Permissões que o host sabe conceder. Qualquer outra recusa o plugin.
/// `modes` só permite registrar modos que restringem (somente leitura); nunca amplia permissão.
pub const KNOWN_PERMISSIONS: &[&str] = &["commands", "hooks", "modes", "cache", "ui", "theme", "autofix"];

/// Conteúdo de `manifest.json`. A pasta do plugin precisa ter o mesmo nome do `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub entry: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PluginError {
    pub folder: String,
    pub message: String,
}

#[derive(Debug, Default, PartialEq, Serialize)]
pub struct PluginScan {
    pub plugins: Vec<Manifest>,
    pub errors: Vec<PluginError>,
}

/// Valida o manifesto e confere se ele corresponde à pasta onde foi encontrado.
pub fn validate(manifest: &Manifest, folder: &str) -> Result<(), String> {
    if !is_valid_id(&manifest.id) {
        return Err(format!(
            "id `{}` inválido: use letras minúsculas, números e hífen, sem começar com hífen",
            manifest.id
        ));
    }
    if manifest.id != folder {
        return Err(format!(
            "id `{}` precisa ser igual ao nome da pasta `{folder}`",
            manifest.id
        ));
    }
    if manifest.name.trim().is_empty() {
        return Err("name é obrigatório".into());
    }
    if !is_safe_relative(&manifest.entry) {
        return Err(format!(
            "entry `{}` precisa ser um caminho relativo dentro da pasta do plugin",
            manifest.entry
        ));
    }
    if let Some(unknown) = manifest
        .permissions
        .iter()
        .find(|p| !KNOWN_PERMISSIONS.contains(&p.as_str()))
    {
        return Err(format!("permissão desconhecida `{unknown}`"));
    }
    Ok(())
}

/// Lista todos os plugins de `plugins_dir`. Um plugin inválido não derruba os outros:
/// o erro vai para `errors` e o restante carrega normalmente.
pub fn scan(plugins_dir: &Path) -> PluginScan {
    let mut result = PluginScan::default();
    let Ok(entries) = fs::read_dir(plugins_dir) else {
        return result;
    };

    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().into_owned();
        match read_manifest(plugins_dir, &folder) {
            Ok(manifest) => result.plugins.push(manifest),
            Err(message) => result.errors.push(PluginError { folder, message }),
        }
    }

    result.plugins.sort_by(|a, b| a.id.cmp(&b.id));
    result
}

/// Lê o arquivo de entrada de um plugin já validado.
pub fn read_entry(plugins_dir: &Path, id: &str) -> Result<String, String> {
    let manifest = read_manifest(plugins_dir, id)?;
    let file = plugins_dir.join(id).join(&manifest.entry);
    fs::read_to_string(&file).map_err(|e| {
        format!(
            "não foi possível ler o entry `{}` do plugin `{id}`: {e}",
            manifest.entry
        )
    })
}

/// Tamanho máximo de um asset de tema.
const ASSET_LIMIT: u64 = 5 * 1024 * 1024;

/// Tipos que um tema pode carregar, pela extensão. Qualquer outro é recusado.
fn asset_mime(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "ogg" => "audio/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => return None,
    })
}

/// Lê um arquivo de `assets/` de um plugin com a permissão `theme`. Devolve o tipo MIME e os bytes.
pub fn read_asset(plugins_dir: &Path, id: &str, rel: &str) -> Result<(&'static str, Vec<u8>), String> {
    let manifest = read_manifest(plugins_dir, id)?;
    if !manifest.permissions.iter().any(|p| p == "theme") {
        return Err(format!("plugin `{id}` não tem a permissão theme"));
    }
    // Só dentro de assets/, sem travessia, sem barra invertida nem unidade de disco.
    let safe = rel.starts_with("assets/")
        && !rel.contains("..")
        && !rel.contains('\\')
        && !rel.contains(':')
        && rel.split('/').all(|part| !part.is_empty());
    if !safe {
        return Err(format!("caminho de asset recusado: `{rel}`"));
    }
    let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let mime = asset_mime(&ext).ok_or_else(|| format!("tipo de arquivo não permitido: `.{ext}`"))?;
    let file = plugins_dir.join(id).join(rel);
    let meta = fs::symlink_metadata(&file).map_err(|e| format!("asset `{rel}` não encontrado: {e}"))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(format!("asset `{rel}` não é um arquivo comum"));
    }
    if meta.len() > ASSET_LIMIT {
        return Err(format!("asset `{rel}` passa de 5 MB"));
    }
    let bytes = fs::read(&file).map_err(|e| format!("não foi possível ler o asset `{rel}`: {e}"))?;
    Ok((mime, bytes))
}

fn read_manifest(plugins_dir: &Path, folder: &str) -> Result<Manifest, String> {
    // Checar o nome da pasta antes de montar o caminho impede travessia (`../`).
    if !is_valid_id(folder) {
        return Err(format!("nome de pasta `{folder}` inválido para um plugin"));
    }

    let file = plugins_dir.join(folder).join("manifest.json");
    let text = fs::read_to_string(&file)
        .map_err(|e| format!("não foi possível ler {}: {e}", file.display()))?;
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|e| format!("manifest.json inválido: {e}"))?;
    validate(&manifest, folder)?;
    Ok(manifest)
}

pub(crate) fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub(crate) fn is_safe_relative(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Cria uma pasta de plugins isolada para cada teste.
    fn temp_plugins_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lunaharness-plugins-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_plugin(dir: &Path, folder: &str, manifest: &str) {
        let plugin = dir.join(folder);
        fs::create_dir_all(&plugin).unwrap();
        fs::write(plugin.join("manifest.json"), manifest).unwrap();
        fs::write(plugin.join("index.js"), "export function activate() {}").unwrap();
    }

    const VALID: &str = r#"{"id":"hello","name":"Hello","version":"0.1.0","entry":"index.js","permissions":["commands","hooks"]}"#;

    #[test]
    fn scans_valid_plugin() {
        let dir = temp_plugins_dir("valid");
        write_plugin(&dir, "hello", VALID);

        let scan = scan(&dir);

        assert!(scan.errors.is_empty());
        assert_eq!(scan.plugins.len(), 1);
        assert_eq!(scan.plugins[0].id, "hello");
    }

    #[test]
    fn accepts_cache_permission() {
        let dir = temp_plugins_dir("cache-permission");
        write_plugin(
            &dir,
            "hello",
            r#"{"id":"hello","name":"Hello","version":"0.1.0","entry":"index.js","permissions":["cache"]}"#,
        );

        let scan = scan(&dir);

        assert!(scan.errors.is_empty());
        assert_eq!(scan.plugins.len(), 1);
    }

    #[test]
    fn id_must_match_folder_name() {
        let dir = temp_plugins_dir("mismatch");
        write_plugin(&dir, "outra-pasta", VALID);

        let scan = scan(&dir);

        assert!(scan.plugins.is_empty());
        assert!(scan.errors[0].message.contains("precisa ser igual ao nome da pasta"));
    }

    #[test]
    fn rejects_entry_outside_plugin_folder() {
        let dir = temp_plugins_dir("traversal-entry");
        write_plugin(
            &dir,
            "hello",
            r#"{"id":"hello","name":"Hello","version":"0.1.0","entry":"../segredo.js"}"#,
        );

        let scan = scan(&dir);

        assert!(scan.plugins.is_empty());
        assert!(scan.errors[0].message.contains("caminho relativo"));
    }

    #[test]
    fn rejects_unknown_permission() {
        let dir = temp_plugins_dir("permission");
        write_plugin(
            &dir,
            "hello",
            r#"{"id":"hello","name":"Hello","version":"0.1.0","entry":"index.js","permissions":["filesystem"]}"#,
        );

        let scan = scan(&dir);

        assert!(scan.errors[0].message.contains("permissão desconhecida `filesystem`"));
    }

    #[test]
    fn invalid_plugin_does_not_block_valid_ones() {
        let dir = temp_plugins_dir("isolation");
        write_plugin(&dir, "hello", VALID);
        write_plugin(&dir, "quebrado", "{ isto não é json");

        let scan = scan(&dir);

        assert_eq!(scan.plugins.len(), 1);
        assert_eq!(scan.errors.len(), 1);
        assert_eq!(scan.errors[0].folder, "quebrado");
    }

    #[test]
    fn read_entry_rejects_path_traversal_in_id() {
        let dir = temp_plugins_dir("traversal-id");

        let result = read_entry(&dir, "../fora");

        assert!(result.unwrap_err().contains("inválido"));
    }

    #[test]
    fn read_entry_returns_source_of_valid_plugin() {
        let dir = temp_plugins_dir("read-entry");
        write_plugin(&dir, "hello", VALID);

        let source = read_entry(&dir, "hello").unwrap();

        assert_eq!(source, "export function activate() {}");
    }
    const THEME: &str = r#"{"id":"tema","name":"Tema","version":"0.1.0","entry":"index.js","permissions":["theme"]}"#;

    fn write_asset(dir: &Path, folder: &str, rel: &str, bytes: &[u8]) {
        let file = dir.join(folder).join(rel);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, bytes).unwrap();
    }

    #[test]
    fn read_asset_returns_mime_and_bytes_for_a_theme_image() {
        let dir = temp_plugins_dir("asset-ok");
        write_plugin(&dir, "tema", THEME);
        write_asset(&dir, "tema", "assets/bg.png", b"PNG");

        let (mime, bytes) = read_asset(&dir, "tema", "assets/bg.png").unwrap();

        assert_eq!(mime, "image/png");
        assert_eq!(bytes, b"PNG");
    }

    #[test]
    fn read_asset_refuses_paths_outside_assets_and_traversal() {
        let dir = temp_plugins_dir("asset-path");
        write_plugin(&dir, "tema", THEME);
        write_asset(&dir, "tema", "assets/bg.png", b"x");

        for rel in ["../tema/manifest.json", "assets/../manifest.json", "/etc/passwd", "C:/x.png", "index.js", "assets\\..\\x.png"] {
            assert!(read_asset(&dir, "tema", rel).is_err(), "deveria recusar {rel}");
        }
    }

    #[test]
    fn read_asset_refuses_types_outside_the_whitelist() {
        let dir = temp_plugins_dir("asset-type");
        write_plugin(&dir, "tema", THEME);
        write_asset(&dir, "tema", "assets/script.html", b"<script>");

        assert!(read_asset(&dir, "tema", "assets/script.html").is_err());
    }

    #[test]
    fn read_asset_needs_the_theme_permission() {
        let dir = temp_plugins_dir("asset-perm");
        write_plugin(&dir, "hello", VALID);
        write_asset(&dir, "hello", "assets/bg.png", b"x");

        assert!(read_asset(&dir, "hello", "assets/bg.png").is_err());
    }
}
