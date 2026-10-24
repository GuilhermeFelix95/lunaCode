//! Importação dos arquivos exportados pelo Chrome ou pelo Edge:
//! HTML de favoritos e histórico (formato Netscape) e CSV de senhas.
//! As senhas ficam criptografadas com a DPAPI do usuário do Windows, sem texto puro no arquivo.
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::browser::parse_url;
use crate::browser_bookmarks::{self, Bookmark};
use crate::commands::app_data_dir;

const PASSWORDS_FILE: &str = "browser-passwords.json";
const MAX_PASSWORDS: usize = 200;
/// Teto de links lidos de um arquivo exportado, para um export gigante não travar o app.
const MAX_LINKS_PER_FILE: usize = 5_000;

/// O papel do arquivo HTML: os favoritos vão para a lista de favoritos; o histórico vai para as visitas.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportTarget {
    Bookmarks,
    History,
}

/// Senha importada e aberta para consulta e cópia.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordOut {
    pub url: String,
    pub login: String,
    pub password: String,
}

/// Como a senha fica no arquivo: só o blob criptografado.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasswordStored {
    url: String,
    login: String,
    blob: String,
}

// ---- DPAPI (Windows): criptografia ligada ao usuário da sessão. ----

#[cfg(windows)]
mod dpapi {
    #![allow(non_snake_case, non_camel_case_types)]
    use core::ffi::c_void;

    #[repr(C)]
    struct CRYPT_INTEGER_BLOB {
        cbData: u32,
        pbData: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            pDataIn: *const CRYPT_INTEGER_BLOB,
            szDataDescr: *const u16,
            pOptionalEntropy: *const c_void,
            pvReserved: *const c_void,
            pPromptStruct: *const c_void,
            dwFlags: u32,
            pDataOut: *mut CRYPT_INTEGER_BLOB,
        ) -> i32;
        fn CryptUnprotectData(
            pDataIn: *const CRYPT_INTEGER_BLOB,
            ppszDataDescr: *mut *mut u16,
            pOptionalEntropy: *const c_void,
            pvReserved: *const c_void,
            pPromptStruct: *const c_void,
            dwFlags: u32,
            pDataOut: *mut CRYPT_INTEGER_BLOB,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(hMem: *mut c_void) -> *mut c_void;
    }

    fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_ptr() as *mut u8 }
    }

    pub fn protect(plain: &str) -> Result<Vec<u8>, String> {
        unsafe {
            let input = blob(plain.as_bytes());
            let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: core::ptr::null_mut() };
            if CryptProtectData(&input, core::ptr::null(), core::ptr::null(), core::ptr::null(), core::ptr::null(), 0, &mut out) == 0 {
                return Err("a DPAPI não aceitou proteger a senha".into());
            }
            let vec = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as *mut c_void);
            Ok(vec)
        }
    }

    pub fn unprotect(blob_bytes: &[u8]) -> Result<String, String> {
        unsafe {
            let input = blob(blob_bytes);
            let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: core::ptr::null_mut() };
            if CryptUnprotectData(&input, core::ptr::null_mut(), core::ptr::null(), core::ptr::null(), core::ptr::null(), 0, &mut out) == 0 {
                return Err("não foi possível desbloquear uma senha importada".into());
            }
            let vec = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as *mut c_void);
            String::from_utf8(vec).map_err(|_| "a senha desbloqueada não é um texto válido".into())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_roundtrip_recovers_the_original_text() {
            let blob = protect("secreta 123!").unwrap();
            assert!(!blob.windows("secreta".len()).any(|chunk| chunk == "secreta".as_bytes()));
            assert_eq!(unprotect(&blob).unwrap(), "secreta 123!");
        }
    }
}

#[cfg(not(windows))]
mod dpapi {
    pub fn protect(_plain: &str) -> Result<Vec<u8>, String> {
        Err("a criptografia de senhas por DPAPI só existe no Windows".into())
    }
}

fn passwords_path(dir: &Path) -> std::path::PathBuf {
    dir.join(PASSWORDS_FILE)
}

fn load_passwords_in(dir: &Path) -> Vec<PasswordStored> {
    fs::read_to_string(passwords_path(dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_passwords_in(dir: &Path, list: &[PasswordStored]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("não foi possível criar a pasta de dados: {e}"))?;
    let text =
        serde_json::to_string_pretty(list).map_err(|e| format!("não foi possível montar o cofre de senhas: {e}"))?;
    let tmp = dir.join(format!("{PASSWORDS_FILE}.tmp"));
    fs::write(&tmp, text).map_err(|e| format!("não foi possível gravar {PASSWORDS_FILE}: {e}"))?;
    fs::rename(&tmp, passwords_path(dir)).map_err(|e| format!("não foi possível salvar {PASSWORDS_FILE}: {e}"))
}

/// Links de um HTML no formato Netscape: pares (endereço, título), sem repetição no arquivo,
/// só http e https (o mesmo filtro do navegador), dentro do limite por arquivo.
pub fn links_in(html: &str) -> Vec<(String, String)> {
    // A busca por marcas usa a versão minúscula: os índices batem com o original,
    // porque to_ascii_lowercase preserva as posições dos bytes.
    let lower = html.to_ascii_lowercase();
    let mut cursor = 0usize;
    let mut out: Vec<(String, String)> = Vec::new();
    while let Some(rel) = lower[cursor..].find("<a") {
        let tag_start = cursor + rel;
        let after = &lower[(tag_start + 2)..];
        // Aceita <A>, <a> e <a href: rejeita palavras que começam por "a", como <aside>.
        if !matches!(after.chars().next(), Some(' ' | '\t' | '\n' | '\r')) {
            cursor = tag_start + 2;
            continue;
        }
        let Some(tag_close_rel) = after.find('>') else { break };
        let tag_end = tag_start + 2 + tag_close_rel;
        let tag = &lower[(tag_start + 2)..tag_end];
        let next = tag_end + 1;

        if let Some(href_rel) = tag.find("href") {
            if let Some(eq_rel) = tag[(href_rel + 4)..].find('=') {
                let eq_abs = tag_start + 2 + href_rel + 4 + eq_rel + 1;
                let raw = &lower[eq_abs..tag_end];
                let trimmed = raw.trim_start();
                let quote_abs = eq_abs + (raw.len() - trimmed.len());
                if let Some(open) = trimmed.chars().next() {
                    if open == '"' || open == '\'' {
                        let value = &lower[(quote_abs + 1)..tag_end];
                        if let Some(close_rel) = value.find(open) {
                            let href = &html[(quote_abs + 1)..(quote_abs + 1 + close_rel)];
                            let title = lower[(tag_end + 1)..]
                                .find("</a")
                                .map(|rel| decode_entities(html[(tag_end + 1)..(tag_end + 1 + rel)].trim()))
                                .unwrap_or_default();
                            if let Ok(parsed) = parse_url(href) {
                                let url = parsed.to_string();
                                if !out.iter().any(|(have, _)| *have == url) {
                                    out.push((url, title));
                                    if out.len() >= MAX_LINKS_PER_FILE {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        cursor = next;
    }
    out
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// Importa os links de um arquivo HTML para favoritos ou histórico. Devolve quantos entraram.
pub fn import_html_in(dir: &Path, html: &str, target: ImportTarget) -> Result<usize, String> {
    let links = links_in(html);
    match target {
        ImportTarget::Bookmarks => import_into_bookmarks(dir, links),
        ImportTarget::History => {
            let mut count = 0;
            for (url, _) in links {
                // Um endereço ruim não derruba o lote inteiro: ele só não entra.
                if crate::browser_history::record_in(dir, &url).is_ok() {
                    count += 1;
                }
            }
            Ok(count)
        }
    }
}

/// Junta os links na lista de favoritos, sem repetir os endereços e respeitando o teto.
fn import_into_bookmarks(dir: &Path, links: Vec<(String, String)>) -> Result<usize, String> {
    let mut list = browser_bookmarks::load_in(dir);
    let mut count = 0;
    for (url, title) in links {
        if list.iter().any(|b| b.url == url) {
            continue;
        }
        if list.len() >= browser_bookmarks::MAX_BOOKMARKS {
            break;
        }
        let id = format!(
            "{:x}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() + count as u128).unwrap_or(0)
        );
        let title = match title.trim() {
            "" => url.clone(),
            typed => typed.chars().take(120).collect(),
        };
        list.push(Bookmark { id, title, url: url.clone() });
        count += 1;
    }
    browser_bookmarks::save_in(dir, &list)?;
    Ok(count)
}

/// Linhas de um CSV com campos entre aspas e vírgulas dentro das aspas
/// (é como o Chrome e o Edge exportam as senhas). As aspas dobradas viram uma aspa só.
pub fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else if c != '\r' {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                }
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    // Linha final vazia (por causa do \\n do fim) não é linha de dados.
    rows.into_iter().filter(|row| row.iter().any(|field| !field.trim().is_empty())).collect()
}

/// Colunas que o CSV precisa ter, achadas pelo nome do cabeçalho.
fn column(rows: &[Vec<String>], names: &[&str]) -> Result<usize, String> {
    let header = rows.first().ok_or("o CSV está vazio")?;
    for (index, cell) in header.iter().enumerate() {
        let cell = cell.trim().to_lowercase();
        if names.iter().any(|name| cell == *name) {
            return Ok(index);
        }
    }
    Err(format!("o CSV não tem a coluna {}; ele precisa ter as colunas url, username e password", names[0]))
}

/// Importa as senhas de um CSV exportado (colunas url, username e password). Devolve quantas entraram.
pub fn import_passwords_in(dir: &Path, csv: &str) -> Result<usize, String> {
    let rows = csv_rows(csv);
    let url_index = column(&rows, &["url"])?;
    let login_index = column(&rows, &["username", "login"])?;
    let password_index = column(&rows, &["password"])?;

    let mut list = load_passwords_in(dir);
    let mut count = 0;
    for row in rows.iter().skip(1) {
        let values = |column: usize| row.get(column).map(|c| c.trim()).unwrap_or("");
        let url = match parse_url(values(url_index)) {
            Ok(parsed) => parsed.to_string(),
            // Linha sem endereço válido não entra, mas não derruba o lote.
            Err(_) => continue,
        };
        let login = values(login_index);
        let password = values(password_index);
        if password.is_empty() {
            continue;
        }
        let blob = base64_encode(&dpapi::protect(password)?);
        list.retain(|entry| !(entry.url == url && entry.login == login));
        list.push(PasswordStored { url, login: login.to_string(), blob });
        count += 1;
        if list.len() > MAX_PASSWORDS {
            list.pop();
            return Err(format!("o cofre aceita até {MAX_PASSWORDS} senhas; ele já está cheio"));
        }
    }
    save_passwords_in(dir, &list)?;
    Ok(count)
}

/// Lista as senhas importadas, já descriptografadas, para consulta e cópia.
pub fn passwords_in(dir: &Path) -> Result<Vec<PasswordOut>, String> {
    load_passwords_in(dir)
        .into_iter()
        .map(|entry| {
            let password = dpapi::unprotect(&base64_decode(&entry.blob)?)?;
            Ok(PasswordOut { url: entry.url, login: entry.login, password })
        })
        .collect()
}

/// Origem web (esquema, host e porta) de um endereço. Endereço que não é http ou https não tem origem.
fn web_origin(url: &str) -> Option<String> {
    parse_url(url).ok().map(|parsed| parsed.origin().ascii_serialization())
}

/// Logins guardados para a origem da página, sem a senha. Só a mesma origem casa.
pub fn login_suggestions_in(dir: &Path, page_url: &str) -> Result<Vec<String>, String> {
    let Some(origin) = web_origin(page_url) else {
        return Ok(vec![]);
    };
    Ok(load_passwords_in(dir)
        .into_iter()
        .filter(|entry| web_origin(&entry.url).as_deref() == Some(origin.as_str()))
        .map(|entry| entry.login)
        .collect())
}

/// Senha de um login, liberada só quando a página está na origem em que ela foi guardada.
pub fn password_for_login_in(dir: &Path, page_url: &str, login: &str) -> Result<String, String> {
    let origin = web_origin(page_url).ok_or("este endereço não recebe senhas")?;
    let entry = load_passwords_in(dir)
        .into_iter()
        .find(|entry| entry.login == login && web_origin(&entry.url).as_deref() == Some(origin.as_str()))
        .ok_or("nenhuma senha salva para este endereço")?;
    dpapi::unprotect(&base64_decode(&entry.blob)?)
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(text).map_err(|_| "o cofre de senhas está corrompido".into())
}

#[tauri::command(async)]
pub fn browser_import_html(app: AppHandle, path: String, target: ImportTarget) -> Result<usize, String> {
    let dir = app_data_dir(&app)?;
    let html = fs::read_to_string(&path).map_err(|e| format!("não foi possível ler {}: {e}", path))?;
    let count = import_html_in(&dir, &html, target)?;
    if matches!(target, ImportTarget::Bookmarks) {
        browser_bookmarks::notify_changed(&app);
    }
    Ok(count)
}

#[tauri::command(async)]
pub fn browser_import_passwords(app: AppHandle, path: String) -> Result<usize, String> {
    let dir = app_data_dir(&app)?;
    let csv = fs::read_to_string(&path).map_err(|e| format!("não foi possível ler {}: {e}", path))?;
    import_passwords_in(&dir, &csv)
}

#[tauri::command(async)]
pub fn browser_passwords(app: AppHandle) -> Result<Vec<PasswordOut>, String> {
    passwords_in(&app_data_dir(&app)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("luna-import-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    const NETSCAPE_HTML: &str = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<HTML><HEAD><TITLE>Bookmarks</TITLE></HEAD><BODY>
<H1>Bookmarks</H1>
<DL><p>
<DT><H3>Trabalho</H3>
<DL><p>
<DT><A HREF="https://vps.exemplo.com/dash" ADD_DATE="1710000000">Painel da VPS</A>
</DL><p>
<DT><A HREF="https://docs.rust-lang.org">Documentação do Rust</A>
<DT><A HREF="file:///C:/local.html">Arquivo local</A>
</DL><p>
</BODY></HTML>"#;

    const PASSWORD_CSV: &str = r#"name,url,username,password
GitHub,https://github.com/login,ana,minha-senha-secreta
"Loja, do Zé",https://lojazee.example.org/login,jo;se,"senha com vírgula"" e aspas"
"#;

    #[test]
    fn reads_the_links_of_a_netscape_html() {
        let links = links_in(NETSCAPE_HTML);

        assert_eq!(
            links,
            vec![
                ("https://vps.exemplo.com/dash".into(), "Painel da VPS".into()),
                ("https://docs.rust-lang.org/".into(), "Documentação do Rust".into()),
            ]
        );
    }

    #[test]
    fn importing_bookmarks_populates_the_bookmark_list() {
        let dir = temp_dir("bookmarks");
        import_html_in(&dir, NETSCAPE_HTML, ImportTarget::Bookmarks).unwrap();

        let bookmarks = browser_bookmarks::load_in(&dir);
        assert_eq!(bookmarks.len(), 2);
        assert!(bookmarks.iter().any(|b: &Bookmark| b.title == "Painel da VPS"));
    }

    #[test]
    fn importing_history_populates_the_visit_list() {
        let dir = temp_dir("history");
        import_html_in(&dir, NETSCAPE_HTML, ImportTarget::History).unwrap();

        let visits = crate::browser_history::load_in(&dir);
        assert_eq!(visits.len(), 2);
        assert!(visits.iter().any(|v| v.url == "https://docs.rust-lang.org/"));
    }

    #[test]
    fn an_empty_or_broken_html_imports_zero() {
        let dir = temp_dir("empty");
        assert_eq!(import_html_in(&dir, "", ImportTarget::Bookmarks).unwrap(), 0);
        assert!(browser_bookmarks::load_in(&dir).is_empty());
    }

    #[test]
    fn reads_csv_rows_with_quoted_commas() {
        let rows = csv_rows(PASSWORD_CSV);

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1][2], "ana");
        assert_eq!(rows[1][3], "minha-senha-secreta");
        assert_eq!(rows[2][3], "senha com vírgula\" e aspas");
        assert_eq!(rows[2][0], "Loja, do Zé");
    }

    #[test]
    fn a_saved_login_is_suggested_only_on_its_exact_origin() {
        let dir = temp_dir("password-origin");
        import_passwords_in(&dir, PASSWORD_CSV).unwrap();

        // Caminho diferente no mesmo site conta; subdomínio, outro domínio e outro esquema não.
        assert_eq!(login_suggestions_in(&dir, "https://github.com/session").unwrap(), vec!["ana".to_string()]);
        assert!(login_suggestions_in(&dir, "https://gist.github.com/login").unwrap().is_empty());
        assert!(login_suggestions_in(&dir, "https://github.com.evil.example/login").unwrap().is_empty());
        assert!(login_suggestions_in(&dir, "http://github.com/login").unwrap().is_empty());
    }

    #[test]
    fn the_password_is_released_only_for_its_exact_origin() {
        let dir = temp_dir("password-release");
        import_passwords_in(&dir, PASSWORD_CSV).unwrap();

        assert_eq!(password_for_login_in(&dir, "https://github.com/login", "ana").unwrap(), "minha-senha-secreta");
        assert!(password_for_login_in(&dir, "https://gist.github.com/login", "ana").is_err());
        assert!(password_for_login_in(&dir, "https://github.com.evil.example/login", "ana").is_err());
        assert!(password_for_login_in(&dir, "https://github.com/login", "outra-pessoa").is_err());
    }

    #[test]
    fn imported_passwords_stay_encrypted_and_come_back_open() {
        let dir = temp_dir("passwords");
        let count = import_passwords_in(&dir, PASSWORD_CSV).unwrap();

        assert_eq!(count, 2);
        // O arquivo não traz a senha em texto puro.
        let raw = fs::read_to_string(passwords_path(&dir)).unwrap();
        assert!(!raw.contains("minha-senha-secreta"));
        assert!(!raw.contains("senha com vírgula"));

        let list = passwords_in(&dir).unwrap();
        assert!(list.iter().any(|p| p.password == "minha-senha-secreta" && p.login == "ana"));
        assert!(list.iter().any(|p| p.password.contains("vírgula") && p.url.contains("lojazee")));
    }

    #[test]
    fn importing_again_replaces_the_same_url_and_login() {
        let dir = temp_dir("dedup");
        import_passwords_in(&dir, PASSWORD_CSV).unwrap();
        let count = import_passwords_in(&dir, PASSWORD_CSV).unwrap();

        assert_eq!(count, 2);
        assert_eq!(passwords_in(&dir).unwrap().len(), 2);
    }
}
