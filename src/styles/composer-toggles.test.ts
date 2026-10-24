import { describe, expect, it } from "vitest";

// Node não tem tipos neste projeto: o módulo entra por nome em variável, sem resolver tipo no tsc.
const FS = "node:fs";
const URL_MODULE = "node:url";
const { readFileSync } = await import(/* @vite-ignore */ FS);
const { fileURLToPath } = await import(/* @vite-ignore */ URL_MODULE);
const read = (name: string): string => readFileSync(fileURLToPath(new URL(name, import.meta.url)), "utf8");

const css = read("./components.css");

describe("alternadores do composer", () => {
  it("o botão meta usa o mesmo estilo do Planejar, com acento quando ligado", () => {
    expect(css).toMatch(/\.composer__plan,[\s\S]*?\.composer__goal,[\s\S]*?\.composer__speed-toggle\s*\{/);
    expect(css).toMatch(/\.composer__goal\[aria-pressed="true"\]/);
    expect(css).toMatch(/\.composer__plan\[aria-pressed="true"\],\s*\.composer__goal\[aria-pressed="true"\]/);
  });
});

describe("remodelação do chat", () => {
  it("o rodapé do composer tem fio separador e as mensagens têm respiro entre turnos", () => {
    expect(blockAfter(".composer__footer {")).toContain("border-top: 1px solid var(--border)");
    expect(blockAfter(".messages {")).toContain("gap: 28px");
  });

  it("a escrita e os balões usam leitura confortável: fonte de 15px no campo e entrelinha de 1.6 nos balões", () => {
    expect(blockAfter(".composer textarea {")).toContain("font-size: 15px");
    expect(blockAfter(".message__bubble {")).toContain("line-height: 1.6");
  });
});

/// Corpo de uma regra, do `{` que segue o seletor até o `}` que fecha.
function blockAfter(marker: string): string {
  const start = css.indexOf(marker);
  if (start < 0) return "";
  const open = css.indexOf("{", start);
  const close = css.indexOf("}", open);
  return css.slice(open + 1, close);
}
