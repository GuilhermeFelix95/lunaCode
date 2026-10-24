import { describe, expect, it } from "vitest";
import { extractQuestions, turnAsksQuestions } from "./questions";

const BLOCK = [
  "Antes de começar, preciso saber:",
  "",
  "```luna-perguntas",
  '[{"pergunta": "Qual banco?", "opcoes": ["Postgres", "SQLite"], "multipla": false},',
  ' {"pergunta": "Quais recursos?", "opcoes": ["Login", "Busca", "Exportar"], "multipla": true}]',
  "```",
].join("\n");

describe("extractQuestions", () => {
  it("tira o bloco do texto e devolve as perguntas", () => {
    const parts = extractQuestions(BLOCK);

    expect(parts.text).toBe("Antes de começar, preciso saber:");
    expect(parts.questions).toEqual([
      { question: "Qual banco?", options: ["Postgres", "SQLite"], multiple: false },
      { question: "Quais recursos?", options: ["Login", "Busca", "Exportar"], multiple: true },
    ]);
    expect(parts.warning).toBeUndefined();
  });

  it("sem bloco, a resposta fica como está", () => {
    const parts = extractQuestions("Plano pronto: criar a tela.");

    expect(parts).toEqual({ text: "Plano pronto: criar a tela.", questions: null });
  });

  it("JSON inválido esconde o bloco e mostra só o aviso", () => {
    const broken = "Pergunta:\n```luna-perguntas\n[{pergunta: sem aspas}]\n```";

    const parts = extractQuestions(broken);

    expect(parts.questions).toBeNull();
    expect(parts.text).toBe("Pergunta:");
    expect(parts.warning).toContain("formato inválido");
  });

  it("bloco sem a cerca de fechamento é escondido e avisa, sem virar seletor", () => {
    const unclosed = 'Pergunta:\n```luna-perguntas\n[{"pergunta": "Oi?", "opcoes": ["Sim", "Não"]}]';

    const parts = extractQuestions(unclosed);

    expect(parts.questions).toBeNull();
    expect(parts.text).toBe("Pergunta:");
    expect(parts.warning).toContain("não foram mostradas");
  });

  it("formato inválido com cerca fechada esconde o bloco e avisa", () => {
    const reply = 'Pergunta:\n```luna-perguntas\n[{"pergunta": "Oi?", "opcoes": ["Sim"]}]\n```';

    const parts = extractQuestions(reply);

    expect(parts.questions).toBeNull();
    expect(parts.text).toBe("Pergunta:");
    expect(parts.warning).toContain("formato inválido");
  });

  it("pergunta com menos de 2 opções é recusada", () => {
    const parts = extractQuestions('```luna-perguntas\n[{"pergunta": "Oi?", "opcoes": ["Sim"], "multipla": false}]\n```');

    expect(parts.questions).toBeNull();
    expect(parts.warning).toBeDefined();
  });

  it("mais de 6 opções também é recusado", () => {
    const options = JSON.stringify(["a", "b", "c", "d", "e", "f", "g"]);
    const parts = extractQuestions('```luna-perguntas\n[{"pergunta": "Letra?", "opcoes": ' + options + "}]\n```");

    expect(parts.questions).toBeNull();
  });

  it("a opção recomendada vem marcada e aparece na lista mesmo se o agente a deixou de fora", () => {
    const parts = extractQuestions(
      '```luna-perguntas\n[{"pergunta": "Banco?", "opcoes": ["Postgres", "MySQL"], "recomendada": "SQLite"}]\n```',
    );

    expect(parts.questions?.[0].recommended).toBe("SQLite");
    expect(parts.questions?.[0].options).toEqual(["SQLite", "Postgres", "MySQL"]);
  });

  it("recomendada que já está entre as opções não duplica nem muda a ordem", () => {
    const parts = extractQuestions(
      '```luna-perguntas\n[{"pergunta": "Banco?", "opcoes": ["Postgres", "SQLite"], "recomendada": "SQLite"}]\n```',
    );

    expect(parts.questions?.[0].options).toEqual(["Postgres", "SQLite"]);
    expect(parts.questions?.[0].recommended).toBe("SQLite");
  });

  it("com 6 opções, a recomendada entra no lugar da última para o limite continuar valendo", () => {
    const options = JSON.stringify(["a", "b", "c", "d", "e", "f"]);
    const parts = extractQuestions('```luna-perguntas\n[{"pergunta": "Letra?", "opcoes": ' + options + ', "recomendada": "z"}]\n```');

    expect(parts.questions?.[0].options).toEqual(["z", "a", "b", "c", "d", "e"]);
  });

  it("o turno faz perguntas no plano, sem modo e com modo que declara questions", () => {
    expect(turnAsksQuestions(true, { id: "x", questions: false })).toBe(true);
    expect(turnAsksQuestions(false, undefined)).toBe(true);
    expect(turnAsksQuestions(false, { id: "revisar", questions: true })).toBe(true);
  });

  it("modo que declara questions false não faz perguntas", () => {
    expect(turnAsksQuestions(false, { id: "revisar", questions: false })).toBe(false);
    expect(turnAsksQuestions(false, { id: "ask", questions: false })).toBe(false);
  });

  it("modo sem o campo questions também faz perguntas pelo seletor", () => {
    expect(turnAsksQuestions(false, { id: "revisar" })).toBe(true);
    expect(turnAsksQuestions(false, { id: "ask" })).toBe(true);
  });

  it("multipla ausente vale false", () => {
    const parts = extractQuestions('```luna-perguntas\n[{"pergunta": "Cor?", "opcoes": ["Azul", "Verde"]}]\n```');

    expect(parts.questions?.[0].multiple).toBe(false);
  });
});
