//! Testes de fumaça com os CLIs reais instalados nesta máquina.
//! Usam sua conta e alguns tokens, por isso ficam `#[ignore]` e só rodam com `npm run test:smoke`.
//! Se um provedor não estiver logado, o teste dele é pulado com aviso, sem falhar.

use std::path::PathBuf;

use lunacode::auth;
use lunacode::providers::{Permission, ProviderEvent, ProviderKind, RunRequest};
use lunacode::session::Session;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lunaharness-smoke-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(
    provider: ProviderKind,
    prompt: &str,
    session_id: Option<String>,
    permission: Permission,
    cwd: PathBuf,
) -> Vec<ProviderEvent> {
    let request = RunRequest {
        provider,
        prompt: prompt.into(),
        session_id,
        cwd,
        permission,
        model: None,
        effort: None,
        browser: Default::default(),
        browser_mode: false,
        web_search: false,
        subagents: false,
        profile: None,
        images: Vec::new(),
        context_window: None,
        long_cache: false,
    };
    let mut session = Session::start(&request).expect("o CLI deve iniciar");
    let mut events = Vec::new();
    session.pump(provider, |e| events.push(e));
    session.finish().expect("o processo deve terminar");
    events
}

fn text_of(events: &[ProviderEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            ProviderEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn session_id_of(events: &[ProviderEvent]) -> Option<String> {
    events.iter().find_map(|e| match e {
        ProviderEvent::SessionStarted { session_id } => Some(session_id.clone()),
        _ => None,
    })
}

/// `true` se o provedor está logado. Imprime aviso e devolve `false` quando não está.
fn logged_in(provider: ProviderKind) -> bool {
    let ok = auth::status(provider).expect("consulta de login deve funcionar");
    if !ok {
        eprintln!("pulado: {provider:?} não está logado nesta máquina");
    }
    ok
}

#[test]
#[ignore]
fn claude_answers_and_finishes_ok() {
    if !logged_in(ProviderKind::Claude) {
        return;
    }
    let events = run(
        ProviderKind::Claude,
        "Responda apenas: ok",
        None,
        Permission::ReadOnly,
        temp_dir("claude-ok"),
    );

    assert!(text_of(&events).to_lowercase().contains("ok"), "eventos: {events:?}");
    assert_eq!(events.last(), Some(&ProviderEvent::Done { ok: true }));
}

#[test]
#[ignore]
fn claude_resume_remembers_previous_turn() {
    if !logged_in(ProviderKind::Claude) {
        return;
    }
    let cwd = temp_dir("claude-resume");
    let first = run(
        ProviderKind::Claude,
        "Guarde o código secreto 4821. Responda apenas: anotado",
        None,
        Permission::ReadOnly,
        cwd.clone(),
    );
    let id = session_id_of(&first).expect("primeira resposta deve trazer o id da sessão");

    let second = run(
        ProviderKind::Claude,
        "Qual é o código secreto? Responda somente o número.",
        Some(id),
        Permission::ReadOnly,
        cwd,
    );

    assert!(text_of(&second).contains("4821"), "eventos: {second:?}");
}

#[test]
#[ignore]
fn read_only_mode_does_not_create_files() {
    if !logged_in(ProviderKind::Claude) {
        return;
    }
    let cwd = temp_dir("claude-readonly");
    let _ = run(
        ProviderKind::Claude,
        "Crie um arquivo chamado teste.txt com o texto oi",
        None,
        Permission::ReadOnly,
        cwd.clone(),
    );

    assert!(!cwd.join("teste.txt").exists(), "modo só leitura não pode criar arquivos");
}

#[test]
#[ignore]
fn codex_answers_and_resume_remembers() {
    if !logged_in(ProviderKind::Codex) {
        return;
    }
    let cwd = temp_dir("codex");
    let first = run(
        ProviderKind::Codex,
        "Guarde o código secreto 4821. Responda apenas: anotado",
        None,
        Permission::ReadOnly,
        cwd.clone(),
    );
    assert_eq!(first.last(), Some(&ProviderEvent::Done { ok: true }), "eventos: {first:?}");
    let id = session_id_of(&first).expect("o Codex deve informar o id da sessão");

    let second = run(
        ProviderKind::Codex,
        "Qual é o código secreto? Responda somente o número.",
        Some(id),
        Permission::ReadOnly,
        cwd,
    );

    assert!(text_of(&second).contains("4821"), "eventos: {second:?}");
}
