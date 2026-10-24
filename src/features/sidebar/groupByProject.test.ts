import { describe, expect, it } from "vitest";
import type { ConversationSummary } from "../../types";
import { folderName, groupByProject } from "./groupByProject";

const now = new Date(2026, 9, 17, 12, 0, 0);
const at = (hours: number) => new Date(2026, 9, 17, hours).getTime();

function conv(id: string, cwd: string, updatedAt: number): ConversationSummary {
  return { id, title: id, provider: "claude", cwd, updatedAt };
}

describe("groupByProject", () => {
  it("separa as conversas por pasta e ordena os projetos pelo nome", () => {
    const groups = groupByProject(
      [
        conv("a", "C:\\LunaHarnes", at(9)),
        conv("b", "C:\\Downloads", at(11)),
        conv("c", "C:\\LunaHarnes", at(10)),
      ],
      now,
    );

    expect(groups.map((g) => g.name)).toEqual(["Downloads", "LunaHarnes"]);
    expect(groups[1].days[0].items.map((c) => c.id)).toEqual(["c", "a"]);
  });

  it("ordena projetos pelas letras base, sem acento: 'Álbum' vem antes de 'Alfa'", () => {
    const groups = groupByProject(
      [conv("a", "C:\\Alfa", at(12)), conv("b", "C:\\Álbum", at(9))],
      now,
    );

    expect(groups.map((g) => g.name)).toEqual(["Álbum", "Alfa"]);
  });

  it("ignora maiúsculas na ordem dos projetos", () => {
    const groups = groupByProject(
      [conv("a", "C:\\zeta", at(9)), conv("b", "C:\\Beta", at(10)), conv("c", "C:\\alfa", at(11))],
      now,
    );

    expect(groups.map((g) => g.name)).toEqual(["alfa", "Beta", "zeta"]);
  });

  it("desempata nomes iguais pelo caminho completo", () => {
    const groups = groupByProject(
      [conv("a", "D:\\trabalho\\app", at(12)), conv("b", "C:\\pessoal\\app", at(9))],
      now,
    );

    expect(groups.map((g) => g.cwd)).toEqual(["C:\\pessoal\\app", "D:\\trabalho\\app"]);
  });

  it("não muda a ordem dos projetos quando uma conversa antiga é usada", () => {
    const groups = groupByProject(
      [conv("a", "C:\\Beta", at(9)), conv("b", "C:\\Alfa", at(10)), conv("c", "C:\\Beta", at(12))],
      now,
    );

    expect(groups.map((g) => g.name)).toEqual(["Alfa", "Beta"]);
  });

  it("não cria projeto para lista vazia", () => {
    expect(groupByProject([], now)).toEqual([]);
  });
});

describe("folderName", () => {
  it("usa o último segmento do caminho, com \\ ou /", () => {
    expect(folderName("C:\\Users\\guilh\\Documents\\LunaHarnes")).toBe("LunaHarnes");
    expect(folderName("/home/guilh/app/")).toBe("app");
  });

  it("raiz de unidade mostra a própria unidade", () => {
    expect(folderName("C:\\")).toBe("C:");
  });
});
