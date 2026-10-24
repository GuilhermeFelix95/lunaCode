import { describe, expect, it } from "vitest";
import { tokensPerSecond } from "./speed";

describe("velocidade da resposta", () => {
  it("divide os tokens de saída medidos pelo tempo desde o primeiro texto", () => {
    // 100 tokens em 2 s = 50 tokens/s
    expect(tokensPerSecond(100, 1000, 3000)).toBe(50);
  });

  it("sem primeiro texto ainda não há taxa", () => {
    expect(tokensPerSecond(100, null, 3000)).toBeNull();
  });

  it("sem tempo decorrido ainda não há taxa", () => {
    expect(tokensPerSecond(100, 3000, 3000)).toBeNull();
  });

  it("sem tokens medidos não há taxa, e zero não aparece como velocidade", () => {
    expect(tokensPerSecond(0, 1000, 2500)).toBeNull();
  });
});
