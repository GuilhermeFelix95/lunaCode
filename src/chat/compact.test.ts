import { describe, expect, it } from "vitest";
import { compactPreamble, COMPACT_PROMPT, needsCompaction } from "./compact";

describe("compactação", () => {
  it("o pedido de resumo pede as seções e não altera arquivos", () => {
    expect(COMPACT_PROMPT).toContain("## Próximos passos");
    expect(COMPACT_PROMPT).toContain("Não altere nenhum arquivo");
  });

  it("o início da sessão nova leva o resumo", () => {
    expect(compactPreamble("## Objetivo\nTela de login.")).toContain("## Objetivo\nTela de login.");
  });

  it("compacta sozinho a partir de 70% da janela", () => {
    expect(needsCompaction(69_000, 100_000)).toBe(false);
    expect(needsCompaction(70_000, 100_000)).toBe(true);
  });

  it("sem janela conhecida, nunca compacta sozinho", () => {
    expect(needsCompaction(500_000, undefined)).toBe(false);
  });
});

describe("sessões depois do corte", () => {
  it("mantém só as sessões criadas depois da compactação, com seen a partir dela", async () => {
    const { sessionsFrom } = await import("./compact");

    expect(sessionsFrom({ claude: { id: "a", seen: 3 }, codex: { id: "b", seen: 9 } }, 5)).toEqual({
      codex: { id: "b", seen: 4 },
    });
  });
});
