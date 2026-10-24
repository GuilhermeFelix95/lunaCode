import { afterEach, describe, expect, it } from "vitest";
import {
  APPROVAL_PROMPT,
  approvalPrompt,
  contextUnderstood,
  extractPlan,
  DEFAULT_PLAN_TARGET,
  loadPlanTarget,
  PLAN_TARGET_KEY,
  planFromReply,
  planInstruction,
  planIsReady,
  QUESTION_PROTOCOL,
  savePlanTarget,
  streamingPlan,
  withPlanInstruction,
} from "./planning";

describe("modo plano", () => {
  it("mantém o pedido do usuário depois da instrução de planejamento", () => {
    const prompt = withPlanInstruction("crie um login");

    expect(prompt.startsWith(planInstruction(DEFAULT_PLAN_TARGET))).toBe(true);
    expect(prompt.endsWith("crie um login")).toBe(true);
  });

  it("a meta de contexto aparece na instrução, e o agente tem de marcar a % em toda resposta", () => {
    const text = planInstruction(65);

    expect(text).toContain("65%");
    expect(text).toContain("Contexto entendido: X%");
  });

  it("a resposta com a pergunta de aprovação vira o plano na aba Plano", () => {
    const approval = {
      text: "Plano: editar o host e o ChatView.",
      questions: [{ question: "Aprovar o plano?", options: ["Aprovar e executar", "Ainda não"], multiple: false }],
    };

    expect(planFromReply(approval, true)).toBe("Plano: editar o host e o ChatView.");
  });

  it("rodada de perguntas não vira plano, e resposta sem planejamento também não", () => {
    const round = { text: "Contexto entendido: 30%", questions: [{ question: "Banco?", options: ["A", "B"], multiple: false }] };

    expect(planFromReply(round, true)).toBeNull();
    expect(planFromReply({ text: "Plano pronto", questions: null }, false)).toBeNull();
    expect(planFromReply({ text: "  ", questions: null }, true)).toBeNull();
  });

  it("só fica pronto o plano que traz o bloco luna-plano ou pede a aprovação", () => {
    const round = { text: "Contexto entendido: 30%", questions: [{ question: "Banco?", options: ["A", "B"], multiple: false }] };
    const approval = { text: "Resumo.", questions: [{ question: "Aprovar?", options: ["Aprovar e executar", "Ainda não"], multiple: false }] };
    const block = { text: "Resumo.\n```luna-plano\n## Passos\n1. Faz\n```", questions: null };

    expect(planIsReady(round)).toBe(false);
    expect(planIsReady({ text: "Resposta solta sem plano", questions: null })).toBe(false);
    expect(planIsReady(approval)).toBe(true);
    expect(planIsReady(block)).toBe(true);
  });

  it("lê a % marcada pelo agente, limitada a 100", () => {
    expect(contextUnderstood("Contexto entendido: 42%\n\nPerguntas abaixo.")).toBe(42);
    expect(contextUnderstood("contexto entendido:  120 %")).toBe(100);
    expect(contextUnderstood("sem a linha")).toBeNull();
  });

  describe("meta de contexto salva", () => {
    afterEach(() => localStorage.removeItem(PLAN_TARGET_KEY));

    it("vale o padrão até o usuário escolher", () => {
      expect(loadPlanTarget()).toBe(DEFAULT_PLAN_TARGET);
    });

    it("guarda a escolha e recusa valores fora de 50 a 100", () => {
      savePlanTarget(90);
      expect(loadPlanTarget()).toBe(90);

      localStorage.setItem(PLAN_TARGET_KEY, "10");
      expect(loadPlanTarget()).toBe(DEFAULT_PLAN_TARGET);
    });
  });

  it("o modo plano ensina o bloco de perguntas, que vira seletor", () => {
    const prompt = withPlanInstruction("crie um login");

    expect(prompt).toContain(QUESTION_PROTOCOL);
    expect(prompt).toContain("luna-perguntas");
  });

  it("a instrução proíbe editar antes da aprovação", () => {
    expect(planInstruction(DEFAULT_PLAN_TARGET)).toContain("Não altere arquivos");
    expect(planInstruction(DEFAULT_PLAN_TARGET)).toContain("Só comece a executar depois que o usuário aprovar");
  });

  it("a aprovação pede execução, sem a instrução de planejamento", () => {
    expect(APPROVAL_PROMPT).toContain("execute o plano agora");
    expect(APPROVAL_PROMPT).not.toContain("modo de planejamento");
  });

  it("a aprovação leva o texto do plano, para a execução não depender só da memória da sessão", () => {
    const prompt = approvalPrompt("## Passos\n1. Criar a tela");
    expect(prompt).toContain("## Passos\n1. Criar a tela");
    expect(prompt).toContain("```luna-plano");
    expect(prompt.startsWith(APPROVAL_PROMPT)).toBe(true);
  });

  it("sem plano guardado, a aprovação sai só com a instrução", () => {
    expect(approvalPrompt(null)).toBe(APPROVAL_PROMPT);
    expect(approvalPrompt("  ")).toBe(APPROVAL_PROMPT);
  });

  it("a spec sai no modelo to-spec, em spec/, com as seções na ordem", () => {
    const sections = ["## Problem Statement", "## Solution", "## User Stories", "## Implementation Decisions", "## Testing Decisions", "## Out of Scope", "## Further Notes"];

    expect(APPROVAL_PROMPT).toContain("spec/");
    const positions = sections.map((section) => APPROVAL_PROMPT.indexOf(section));
    expect(positions.every((position) => position >= 0)).toBe(true);
    expect([...positions].sort((a, b) => a - b)).toEqual(positions);
  });
});

describe("extractPlan", () => {
  it("separa o resumo do plano completo", () => {
    const reply = "Resumo em três linhas.\nVou criar a tela.\nFalta só aprovar.\n```luna-plano\n## Contexto\nTarefas.\n## Arquivos a alterar\n- src/App.tsx: cria a tela\n```";

    const parts = extractPlan(reply);

    expect(parts?.summary).toBe("Resumo em três linhas.\nVou criar a tela.\nFalta só aprovar.");
    expect(parts?.plan).toContain("## Arquivos a alterar");
  });

  it("sem bloco ou com bloco vazio, não há plano separado", () => {
    expect(extractPlan("Só um texto.")).toBeNull();
    expect(extractPlan("```luna-plano\n   \n```")).toBeNull();
  });

  it("o prompt do modo plano pede o resumo e o bloco luna-plano", () => {
    const text = planInstruction(DEFAULT_PLAN_TARGET);

    expect(text).toContain("luna-plano");
    expect(text).toContain("## Não mexer");
  });
});

describe("streamingPlan", () => {
  it("sem o bloco luna-plano, ainda não há plano para mostrar", () => {
    expect(streamingPlan("Contexto entendido: 40%\nPreciso saber mais.")).toBeNull();
  });

  it("mostra o plano que já chegou, sem a cerca de abertura", () => {
    const reply = "Resumo.\n```luna-plano\n## Contexto\nAinda chega";
    expect(streamingPlan(reply)).toBe("## Contexto\nAinda chega");
  });

  it("tira a cerca de fechamento quando o bloco termina", () => {
    const reply = "Resumo.\n```luna-plano\n## Passos\n1. Faz\n```";
    expect(streamingPlan(reply)).toBe("## Passos\n1. Faz");
  });

  it("tira a cerca parcial, ainda chegando", () => {
    const reply = "```luna-plano\n## Riscos\nNenhum\n``";
    expect(streamingPlan(reply)).toBe("## Riscos\nNenhum");
  });
});

describe("approvalPrompt por motor", () => {
  it("o Claude registra as etapas com TaskCreate, e não cita o servidor de etapas", () => {
    const prompt = approvalPrompt("## Passos\n1. Criar a tela", "claude");
    expect(prompt).toContain("TaskCreate");
    expect(prompt).not.toContain("lunatasks");
  });

  it("o OpenCode registra as etapas com lunatasks.set_steps, e não cita TaskCreate", () => {
    const prompt = approvalPrompt("## Passos\n1. Criar a tela", "opencode");
    expect(prompt).toContain("tools.lunatasks.set_steps");
    expect(prompt).toContain("content");
    expect(prompt).not.toContain("TaskCreate");
  });

  it("o Copilot não recebe instrução de etapas, porque ainda não tem o servidor do Luna", () => {
    const prompt = approvalPrompt("## Passos\n1. Criar a tela", "copilot");
    expect(prompt).not.toContain("TaskCreate");
    expect(prompt).not.toContain("lunatasks");
    expect(prompt).toContain("execute o plano agora");
  });

  it("o Antigravity também não recebe instrução de etapas, pelo mesmo motivo do Copilot", () => {
    const prompt = approvalPrompt("## Passos\n1. Criar a tela", "antigravity");
    expect(prompt).not.toContain("TaskCreate");
    expect(prompt).not.toContain("lunatasks");
    expect(prompt).toContain("execute o plano agora");
  });

  it("o Codex também usa o servidor de etapas, porque não tem TaskCreate", () => {
    const prompt = approvalPrompt("## Passos\n1. Criar a tela", "codex");
    expect(prompt).toContain("tools.lunatasks.set_steps");
    expect(prompt).not.toContain("TaskCreate");
  });
});

describe("aprovação com pasta de spec", () => {
  it("manda gravar a spec dentro da pasta da conversa, não em spec/", () => {
    const prompt = approvalPrompt("## Passos\n1. Criar a tela", "claude", "specs/001-tela");
    expect(prompt).toContain("em specs/001-tela/spec.md");
    expect(prompt).not.toContain("em spec/ na raiz");
  });

  it("sem pasta, o texto continua o de sempre", () => {
    expect(approvalPrompt(null, "claude")).toBe(APPROVAL_PROMPT);
  });
});

describe("aprovação pede para fechar todas as etapas", () => {
  it("o Claude marca todas as etapas como concluídas antes de terminar", () => {
    expect(approvalPrompt(null, "claude")).toMatch(/Antes de terminar, marque todas as etapas como completed/);
  });

  it("OpenCode e Codex fecham todas as etapas pelo set_steps antes de terminar", () => {
    for (const provider of ["opencode", "codex"] as const) {
      expect(approvalPrompt(null, provider)).toMatch(/Antes de terminar, marque todas as etapas como completed/);
    }
  });

  it("Copilot e Antigravity, sem registro de etapas, não recebem a instrução", () => {
    for (const provider of ["copilot", "antigravity"] as const) {
      expect(approvalPrompt(null, provider)).not.toMatch(/marque todas as etapas/);
    }
  });
});
