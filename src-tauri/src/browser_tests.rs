//! Página local de teste para o fluxo de tarefas no navegador (preencher o texto e publicar).
//! Só os testes sobem o servidor: ele responde a fixture HTML em 127.0.0.1, numa porta livre.
use std::cell::{Cell, RefCell};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use serde_json::{json, Value};

use crate::browser::{act_with, ActionGate, BrowserPage};

const FIXTURE: &str = include_str!("browser_fixtures/publicar.html");

/// Sobe o servidor da fixture numa porta livre e devolve o endereço da página.
fn serve_fixture() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("porta livre para o teste");
    let address = listener.local_addr().expect("endereço do teste");
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            respond(stream);
        }
    });
    format!("http://{address}/")
}

/// Responde qualquer pedido com a fixture e fecha a conexão.
fn respond(mut stream: TcpStream) {
    let mut request = [0u8; 2048];
    let _ = stream.read(&mut request);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{FIXTURE}",
        FIXTURE.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

/// Pede a página ao servidor de teste e devolve a resposta completa.
fn fetch(url: &str) -> String {
    let address = url.trim_start_matches("http://").trim_end_matches('/');
    let mut stream = TcpStream::connect(address).expect("conexão com a página de teste");
    stream.write_all(format!("GET / HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n").as_bytes()).expect("pedido enviado");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("resposta lida");
    response
}

#[test]
fn the_local_test_page_is_served_with_the_publish_button() {
    let response = fetch(&serve_fixture());
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains("id=\"botao-publicar\""), "{response}");
    assert!(response.contains(">Publicar</button>"), "{response}");
}

/// Página falsa com o estado da fixture: o campo de texto (ref 1) e o botão Publicar (ref 2).
struct PaginaDeTeste {
    texto: RefCell<String>,
    publicado: Cell<bool>,
}

impl PaginaDeTeste {
    fn nova() -> Self {
        Self { texto: RefCell::new(String::new()), publicado: Cell::new(false) }
    }
}

impl BrowserPage for PaginaDeTeste {
    fn reply(&self, method: &str, args: &[Value]) -> Result<Value, String> {
        match method {
            "inspect" => {
                let name = if args[0].as_u64() == Some(1) { "Texto" } else { "Publicar" };
                Ok(json!({ "ok": true, "name": name }))
            }
            "act" => {
                match args[1].as_str() {
                    Some("type") => *self.texto.borrow_mut() = args[2].as_str().unwrap_or_default().to_string(),
                    Some("click") => self.publicado.set(true),
                    _ => {}
                }
                Ok(json!({ "ok": true }))
            }
            other => Err(format!("método inesperado no teste: {other}")),
        }
    }
}

/// Permissão falsa: a pessoa aprova ou nega a ação sensível.
struct Permissao(bool);

impl ActionGate for Permissao {
    fn confirm(&self, _run_id: &str, _action: &str, _name: &str) -> Result<(), String> {
        if self.0 { Ok(()) } else { Err("negado pela pessoa no app".into()) }
    }
}

#[test]
fn the_agent_fills_the_text_field_without_asking() {
    let page = PaginaDeTeste::nova();
    let result = act_with(&page, &Permissao(false), "run-teste", "type", &json!({ "ref": 1, "text": "Texto melhorado" }));
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(page.texto.borrow().as_str(), "Texto melhorado");
    assert!(!page.publicado.get());
}

#[test]
fn publishing_without_the_persons_permission_publishes_nothing() {
    let page = PaginaDeTeste::nova();
    let result = act_with(&page, &Permissao(false), "run-teste", "click", &json!({ "ref": 2 }));
    assert!(result.is_err());
    assert!(!page.publicado.get());
}

#[test]
fn publishing_with_the_persons_permission_publishes_the_text() {
    let page = PaginaDeTeste::nova();
    act_with(&page, &Permissao(false), "run-teste", "type", &json!({ "ref": 1, "text": "Texto melhorado" })).unwrap();
    let result = act_with(&page, &Permissao(true), "run-teste", "click", &json!({ "ref": 2 }));
    assert!(result.is_ok(), "{result:?}");
    assert!(page.publicado.get());
}
