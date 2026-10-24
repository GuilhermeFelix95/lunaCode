import { describe, expect, it } from "vitest";

import { parseArgs, pickPageTarget } from "./spike-upload-cdp.mjs";

describe("pickPageTarget", () => {
  it("escolhe a aba de página da fixture de upload", () => {
    const abas = [
      { type: "page", url: "https://example.com", title: "Outra" },
      { type: "page", url: "file:///C:/app/upload.html", title: "x" },
    ];
    expect(pickPageTarget(abas)).toBe(abas[1]);
  });

  it("ignora abas que não são página", () => {
    const abas = [{ type: "service_worker", url: "file:///upload.html", title: "" }];
    expect(pickPageTarget(abas)).toBeNull();
  });

  it("retorna null sem aba correspondente", () => {
    expect(pickPageTarget([])).toBeNull();
  });
});

describe("parseArgs", () => {
  it("usa a porta 9222 por padrão", () => {
    expect(parseArgs([])).toEqual({ port: 9222, file: null });
  });

  it("lê porta e arquivo", () => {
    expect(parseArgs(["--port", "9333", "--file", "a.txt"])).toEqual({ port: 9333, file: "a.txt" });
  });

  it("recusa argumento desconhecido e porta inválida", () => {
    expect(() => parseArgs(["--x", "1"])).toThrow(/desconhecido/);
    expect(() => parseArgs(["--port", "abc"])).toThrow(/Porta inválida/);
  });
});
