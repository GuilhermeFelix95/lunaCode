import { describe, expect, it } from "vitest";
import type { CliStatus } from "../../types";
import { needsCliSetup } from "./needsCliSetup";

const installed: CliStatus = { provider: "claude", installed: true, path: "C:\\claude.exe" };
const missing: CliStatus = { provider: "codex", installed: false, path: null };

describe("needsCliSetup", () => {
  it("não abre com lista vazia", () => {
    expect(needsCliSetup([])).toBe(false);
  });

  it("não abre quando todas estão instaladas", () => {
    expect(needsCliSetup([installed, { ...installed, provider: "codex" }])).toBe(false);
  });

  it("não abre quando pelo menos uma está instalada", () => {
    expect(needsCliSetup([installed, missing])).toBe(false);
  });

  it("abre quando nenhuma está instalada", () => {
    expect(needsCliSetup([missing, { ...missing, provider: "opencode" }])).toBe(true);
  });
});
