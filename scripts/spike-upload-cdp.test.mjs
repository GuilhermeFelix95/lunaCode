import assert from "node:assert/strict";
import { test } from "node:test";

import { parseArgs, pickPageTarget } from "./spike-upload-cdp.mjs";

test("pickPageTarget escolhe a aba de página da fixture de upload", () => {
  const abas = [
    { type: "page", url: "https://example.com", title: "Outra" },
    { type: "page", url: "file:///C:/app/upload.html", title: "x" },
  ];
  assert.equal(pickPageTarget(abas), abas[1]);
});

test("pickPageTarget ignora abas que não são página", () => {
  const abas = [{ type: "service_worker", url: "file:///upload.html", title: "" }];
  assert.equal(pickPageTarget(abas), null);
});

test("pickPageTarget retorna null sem aba correspondente", () => {
  assert.equal(pickPageTarget([]), null);
});

test("parseArgs usa a porta 9222 por padrão", () => {
  assert.deepEqual(parseArgs([]), { port: 9222, file: null });
});

test("parseArgs lê porta e arquivo", () => {
  assert.deepEqual(parseArgs(["--port", "9333", "--file", "a.txt"]), { port: 9333, file: "a.txt" });
});

test("parseArgs recusa argumento desconhecido e porta inválida", () => {
  assert.throws(() => parseArgs(["--x", "1"]), /desconhecido/);
  assert.throws(() => parseArgs(["--port", "abc"]), /Porta inválida/);
});
