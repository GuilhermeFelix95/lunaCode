//! Ações do agente sobre a página do navegador: leitura numerada, cliques, teclas, rolagem, espera e pesquisa.
//! O JavaScript que roda na página fica em `browser_scripts/page.js`; aqui ficam as partes que não dependem
//! da webview (montagem das chamadas, classificação de ações sensíveis, bloqueios, motores e a leitura do snapshot).
use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::Url;

/// Script da página, colado antes de cada chamada. Ver o cabeçalho do arquivo.
pub const PAGE_SCRIPT: &str = include_str!("browser_scripts/page.js");

/// Teclas que o agente pode apertar. Qualquer outra é recusada antes de chegar à página.
pub const KEYS: [&str; 7] = ["Escape", "Enter", "Tab", "ArrowDown", "ArrowUp", "PageDown", "PageUp"];

/// Motores de busca que o app oferece (Configurações > Navegador), com a ordem de tentativa.
pub const ENGINES: [&str; 4] = ["google", "duckduckgo", "bing", "brave"];

/// Resultado mínimo para aceitar uma página de busca. Abaixo disso, a pesquisa tenta o próximo motor.
const MIN_RESULTS: usize = 3;
const MAX_QUERY: usize = 200;

/// Palavras que tornam uma ação sensível: comprar, pagar, enviar, publicar, apagar ou confirmar.
const SENSITIVE_WORDS: [&str; 26] = [
    "comprar", "compra", "pagar", "pagamento", "checkout", "buy", "pay", "enviar", "envie", "send", "publicar", "publish", "postar", "post",
    "excluir", "apagar", "remover", "delete", "remove", "confirmar", "confirm", "submit", "transferir", "transfer", "assinar", "subscribe",
];

/// Motor escolhido em Configurações > Navegador. O front manda a escolha ao mudar; o padrão é o Google.
static SEARCH_ENGINE: Mutex<Option<String>> = Mutex::new(None);

/// Chamada de um método do script da página. Os argumentos entram como JSON, então não há injeção.
pub fn page_call(method: &str, args: &[Value]) -> String {
    let args: Vec<String> = args.iter().map(Value::to_string).collect();
    format!("(() => {{ {PAGE_SCRIPT}\nreturn JSON.stringify(__luna.{method}({})); }})()", args.join(", "))
}

/// Ação sensível pelo nome do elemento: qualquer palavra de `SENSITIVE_WORDS` no nome, sem diferença de caixa.
pub fn is_sensitive(name: &str) -> bool {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| SENSITIVE_WORDS.contains(&word))
}

/// Texto que o agente recebe quando a página para em login, CAPTCHA ou confirmação. Ele deve parar e pedir ajuda.
pub fn blocker_message(kind: &str) -> String {
    match kind {
        "login" => "BLOQUEIO: a página pede login. Pare, não tente contornar, e peça à pessoa que entre na página.".into(),
        "captcha" => "BLOQUEIO: a página pede verificação (CAPTCHA). Pare, e peça à pessoa que resolva na página.".into(),
        "confirmacao" => "BLOQUEIO: a página pede confirmação de conta ou código. Pare, e peça à pessoa que confirme na página.".into(),
        other => format!("BLOQUEIO: {other}. Pare e peça ajuda à pessoa."),
    }
}

/// Endereço de busca de um motor. Motor desconhecido não tem endereço.
pub fn search_url(engine: &str, query: &str) -> Result<String, String> {
    let base = match engine {
        "google" => "https://www.google.com/search",
        "duckduckgo" => "https://duckduckgo.com/",
        "bing" => "https://www.bing.com/search",
        "brave" => "https://search.brave.com/search",
        other => return Err(format!("motor de busca desconhecido: {other}")),
    };
    let url = Url::parse_with_params(base, &[("q", query.trim())]).map_err(|e| e.to_string())?;
    Ok(url.to_string())
}

/// Ordem de tentativa: o motor escolhido primeiro, depois os outros na ordem do app.
pub fn engine_order(preferred: &str) -> Vec<&'static str> {
    let first = ENGINES.iter().copied().find(|engine| *engine == preferred).unwrap_or(ENGINES[0]);
    std::iter::once(first).chain(ENGINES.iter().copied().filter(|engine| *engine != first)).collect()
}

/// Um motor serve quando trouxe links suficientes para outros sites.
pub fn has_results(links: &Value) -> bool {
    links.as_array().is_some_and(|items| items.len() >= MIN_RESULTS)
}

/// Texto de resultados de busca: uma linha por link, com o texto e o endereço.
pub fn render_links(links: &Value) -> String {
    links
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, link)| format!("{}. {} — {}", index + 1, link["text"].as_str().unwrap_or_default(), link["href"].as_str().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Valida o pedido de busca e devolve o termo limpo.
pub fn clean_query(query: &str) -> Result<String, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("informe o que pesquisar".into());
    }
    if trimmed.chars().count() > MAX_QUERY {
        return Err(format!("a pesquisa passa de {MAX_QUERY} caracteres"));
    }
    Ok(trimmed.to_string())
}

/// Tecla que o agente pediu, se estiver na lista permitida.
pub fn checked_key(key: &str) -> Result<&str, String> {
    KEYS.iter().copied().find(|allowed| *allowed == key).ok_or_else(|| format!("tecla não permitida: {key}"))
}

/// Guarda o motor escolhido no app. Motor fora da lista é recusado.
pub fn set_search_engine(engine: &str) -> Result<(), String> {
    if !ENGINES.contains(&engine) {
        return Err(format!("motor de busca desconhecido: {engine}"));
    }
    *SEARCH_ENGINE.lock().map_err(|_| "estado do navegador travado".to_string())? = Some(engine.to_string());
    Ok(())
}

/// Motor escolhido, ou o padrão do app quando a pessoa ainda não escolheu nada.
pub fn search_engine() -> String {
    SEARCH_ENGINE.lock().ok().and_then(|guard| guard.clone()).unwrap_or_else(|| ENGINES[0].to_string())
}

/// Lista numerada do snapshot, que o agente lê para escolher a referência de cada ação.
pub fn render_snapshot(snapshot: &Value) -> String {
    let mut lines = vec![
        format!("Endereço: {}", snapshot["url"].as_str().unwrap_or_default()),
        format!("Título: {}", snapshot["title"].as_str().unwrap_or_default()),
    ];
    if let Some(kind) = snapshot["blocker"].as_str() {
        lines.push(blocker_message(kind));
    }
    lines.push("Elementos (use a referência em browser_click, browser_type e browser_press_key):".into());
    for item in snapshot["items"].as_array().into_iter().flatten() {
        let dialog = item["dialog"].as_str().map(|title| format!(" (diálogo: {title})")).unwrap_or_default();
        let disabled = if item["disabled"].as_bool() == Some(true) { " [desativado]" } else { "" };
        lines.push(format!(
            "[{}] {} \"{}\"{dialog}{disabled}",
            item["ref"].as_u64().unwrap_or_default(),
            item["role"].as_str().unwrap_or_default(),
            item["name"].as_str().unwrap_or_default(),
        ));
    }
    if snapshot["items"].as_array().is_none_or(|items| items.is_empty()) {
        lines.push("(nenhum elemento interativo visível)".into());
    }
    lines.join("\n")
}

/// Resposta de uma chamada do script: o JSON é devolvido pela página como texto.
pub fn parse_page_reply(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|e| format!("resposta inesperada da página: {e}"))
}

/// Campo `error` de uma resposta com `ok: false`, ou uma mensagem genérica.
pub fn page_error(reply: &Value) -> String {
    reply["error"].as_str().unwrap_or("a página não concluiu a ação").to_string()
}

/// Corpo do pedido de confirmação para uma ação sensível.
pub fn confirmation_input(action: &str, name: &str) -> Value {
    json!({ "ação": action, "elemento": name })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_call_passes_arguments_as_literals() {
        let script = page_call("act", &[json!(7), json!("type"), json!("olá \"x\"")]);
        assert!(script.contains("__luna.act(7, \"type\", \"olá \\\"x\\\"\")"));
        assert!(script.contains("const __luna"));
    }

    #[test]
    fn sensitive_actions_are_recognised_by_their_words_in_any_case() {
        assert!(is_sensitive("Comprar agora"));
        assert!(is_sensitive("Finalizar pedido e PAGAR"));
        assert!(is_sensitive("Publicar"));
        assert!(is_sensitive("Excluir conta"));
        assert!(is_sensitive("Submit"));
        assert!(is_sensitive("Postar"));
        assert!(is_sensitive("Post"));
        assert!(!is_sensitive("Fechar"));
        assert!(!is_sensitive("Inscrever-se"));
        // Palavra dentro de outra não conta: "compartilhar" não é "compra".
        assert!(!is_sensitive("Compartilhar"));
    }

    #[test]
    fn each_blocker_tells_the_agent_to_stop() {
        for kind in ["login", "captcha", "confirmacao"] {
            assert!(blocker_message(kind).starts_with("BLOQUEIO:"), "{kind}");
            assert!(blocker_message(kind).contains("Pare"), "{kind}");
        }
    }

    #[test]
    fn search_urls_use_the_engine_and_encode_the_query() {
        assert_eq!(search_url("google", "rust async").unwrap(), "https://www.google.com/search?q=rust+async");
        assert_eq!(search_url("duckduckgo", "gatos & cães").unwrap(), "https://duckduckgo.com/?q=gatos+%26+c%C3%A3es");
        assert_eq!(search_url("brave", " x ").unwrap(), "https://search.brave.com/search?q=x");
        assert!(search_url("yahoo", "x").is_err());
    }

    #[test]
    fn the_chosen_engine_is_tried_first_then_the_others_in_app_order() {
        assert_eq!(engine_order("bing"), vec!["bing", "google", "duckduckgo", "brave"]);
        assert_eq!(engine_order("google"), vec!["google", "duckduckgo", "bing", "brave"]);
        // Motor inválido cai no padrão.
        assert_eq!(engine_order("yahoo")[0], "google");
    }

    #[test]
    fn a_search_needs_at_least_three_links_to_count_as_results() {
        assert!(!has_results(&json!([])));
        assert!(!has_results(&json!([{ "text": "a", "href": "https://a" }, { "text": "b", "href": "https://b" }])));
        assert!(has_results(&json!([
            { "text": "a", "href": "https://a" },
            { "text": "b", "href": "https://b" },
            { "text": "c", "href": "https://c" }
        ])));
    }

    #[test]
    fn only_listed_keys_reach_the_page() {
        assert_eq!(checked_key("Escape").unwrap(), "Escape");
        assert!(checked_key("F5").is_err());
        assert!(checked_key("Escape;alert(1)").is_err());
    }

    #[test]
    fn queries_are_trimmed_and_limited() {
        assert_eq!(clean_query("  repo  ").unwrap(), "repo");
        assert!(clean_query("   ").is_err());
        assert!(clean_query(&"a".repeat(MAX_QUERY + 1)).is_err());
    }

    #[test]
    fn the_snapshot_text_shows_references_dialogs_and_blockers() {
        let snapshot = json!({
            "url": "https://youtube.com/@x",
            "title": "Canal",
            "blocker": null,
            "items": [
                { "ref": 1, "role": "button", "name": "Fechar", "dialog": "Descrição", "disabled": false },
                { "ref": 2, "role": "link", "name": "Início", "dialog": null, "disabled": true }
            ]
        });
        let text = render_snapshot(&snapshot);
        assert!(text.contains("[1] button \"Fechar\" (diálogo: Descrição)"), "{text}");
        assert!(text.contains("[2] link \"Início\" [desativado]"), "{text}");
        assert!(!text.contains("BLOQUEIO"));
    }

    #[test]
    fn a_blocked_page_says_so_in_the_snapshot() {
        let text = render_snapshot(&json!({ "url": "u", "title": "t", "blocker": "login", "items": [] }));
        assert!(text.contains("BLOQUEIO: a página pede login"));
        assert!(text.contains("nenhum elemento interativo"));
    }
}
