// Spike de envio de arquivo via CDP (DevTools Protocol) no WebView2 do LunaCode.
//
// Uso:
//   1. Abra o app com depuração remota:
//      set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
//      npm run tauri dev
//   2. Sirva a fixture em http, porque o navegador do app só aceita http e https:
//      python -m http.server 8765 --bind 127.0.0.1 -d src-tauri/src/browser_fixtures
//      e abra http://127.0.0.1:8765/upload.html no navegador do app.
//   3. Rode: node scripts/spike-upload-cdp.mjs [--port 9222] [--file caminho]
//
// Sai com código 0 quando o input recebe o arquivo, e com 1 caso contrário.

import { readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const PAGE_HINT = "upload.html";
const FILE_INPUT_SELECTOR = "#arquivo";
const RESULT_SELECTOR = "#resultado";
const DEFAULT_PORT = 9222;

/**
 * Escolhe a aba de página que contém a fixture de upload.
 * Retorna null quando não há nenhuma aba correspondente.
 */
export function pickPageTarget(targets) {
  return (
    targets.find(
      (target) =>
        target.type === "page" &&
        (target.url.includes(PAGE_HINT) || target.title.includes("upload")),
    ) ?? null
  );
}

/**
 * Lê os argumentos `--port` e `--file` da linha de comando.
 */
export function parseArgs(argv) {
  const options = { port: DEFAULT_PORT, file: null };
  for (let i = 0; i < argv.length; i += 2) {
    const valor = argv[i + 1];
    if (argv[i] === "--port") options.port = Number(valor);
    else if (argv[i] === "--file") options.file = valor;
    else throw new Error(`Argumento desconhecido: ${argv[i]}`);
    if (valor === undefined) throw new Error(`Falta valor para ${argv[i]}`);
  }
  if (!Number.isInteger(options.port) || options.port <= 0) {
    throw new Error(`Porta inválida: ${options.port}`);
  }
  return options;
}

/**
 * Cliente mínimo de CDP: envia comandos JSON e espera a resposta pelo id.
 */
export function createCdpClient(webSocketUrl) {
  const socket = new WebSocket(webSocketUrl);
  const pendentes = new Map();
  let proximoId = 1;

  socket.addEventListener("message", (evento) => {
    const mensagem = JSON.parse(evento.data);
    const pendente = pendentes.get(mensagem.id);
    if (!pendente) return;
    pendentes.delete(mensagem.id);
    if (mensagem.error) pendente.reject(new Error(mensagem.error.message));
    else pendente.resolve(mensagem.result);
  });

  const aberto = new Promise((resolver, rejeitar) => {
    socket.addEventListener("open", resolver, { once: true });
    socket.addEventListener("error", () => rejeitar(new Error("Falha ao abrir o WebSocket do CDP")), { once: true });
  });

  return {
    aberto,
    send(method, params = {}) {
      const id = proximoId++;
      return new Promise((resolve, reject) => {
        pendentes.set(id, { resolve, reject });
        socket.send(JSON.stringify({ id, method, params }));
      });
    },
    close() {
      socket.close();
    },
  };
}

async function listarAbas(port) {
  const resposta = await fetch(`http://127.0.0.1:${port}/json/list`);
  if (!resposta.ok) {
    throw new Error(`CDP respondeu ${resposta.status}. O app está com --remote-debugging-port=${port}?`);
  }
  return resposta.json();
}

async function enviarArquivo(client, caminhoArquivo) {
  const { root } = await client.send("DOM.getDocument", { depth: -1 });
  const { nodeId } = await client.send("DOM.querySelector", {
    nodeId: root.nodeId,
    selector: FILE_INPUT_SELECTOR,
  });
  if (!nodeId) throw new Error(`Input ${FILE_INPUT_SELECTOR} não encontrado na página`);
  await client.send("DOM.setFileInputFiles", { nodeId, files: [caminhoArquivo] });
}

async function lerResultado(client) {
  const { result } = await client.send("Runtime.evaluate", {
    expression: `document.querySelector("${RESULT_SELECTOR}")?.textContent ?? ""`,
    returnByValue: true,
  });
  return result.value;
}

async function criarArquivoDeTeste() {
  const caminho = join(tmpdir(), "luna-spike-upload.txt");
  await writeFile(caminho, "conteudo de teste do spike de upload\n");
  return caminho;
}

async function main() {
  const { port, file } = parseArgs(process.argv.slice(2));
  const caminhoArquivo = file ? resolve(file) : await criarArquivoDeTeste();
  await readFile(caminhoArquivo);

  const alvo = pickPageTarget(await listarAbas(port));
  if (!alvo) {
    throw new Error(`Nenhuma aba com ${PAGE_HINT} aberta. Abra a fixture no navegador do app e rode de novo.`);
  }

  const client = createCdpClient(alvo.webSocketDebuggerUrl);
  await client.aberto;
  try {
    await client.send("DOM.enable");
    await client.send("Runtime.enable");
    await enviarArquivo(client, caminhoArquivo);
    const resultado = await lerResultado(client);
    const nomeEsperado = caminhoArquivo.split(/[\\/]/).pop();
    const recebeu = resultado.startsWith(`recebido:${nomeEsperado}:`);
    console.log(recebeu ? "SPIKE OK: o input recebeu o arquivo." : "SPIKE FALHOU: o input não recebeu o arquivo.");
    console.log(`Resultado da página: "${resultado}"`);
    console.log(`Página: ${alvo.url}`);
    process.exitCode = recebeu ? 0 : 1;
  } finally {
    client.close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((erro) => {
    console.error(`SPIKE ERRO: ${erro.message}`);
    process.exitCode = 1;
  });
}
