//! Página local de teste para o fluxo de tarefas no navegador (preencher o texto e publicar).
//! Só os testes sobem o servidor: ele responde a fixture HTML em 127.0.0.1, numa porta livre.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

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
