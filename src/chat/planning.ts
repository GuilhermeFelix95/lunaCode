/// Modo plano: o agente entrevista, planeja e só depois edita. Vale para qualquer CLI,
/// porque o Codex não tem modo de plano próprio: a regra vai no prompt e a sandbox fica só leitura.
import type { ProviderKind } from "../types";
import type { Question } from "./questions";

/// Meta padrão: o plano só sai quando o agente diz que tem pelo menos este tanto do contexto.
export const DEFAULT_PLAN_TARGET = 80;
export const PLAN_TARGET_KEY = "luna.planTarget";

/// Meta de contexto (%) para montar o plano, escolhida em Configurações. Fora de 50–100, vale o padrão.
export function loadPlanTarget(): number {
  try {
    const saved = Number(localStorage.getItem(PLAN_TARGET_KEY));
    if (Number.isInteger(saved) && saved >= 50 && saved <= 100) return saved;
  } catch {
    // Armazenamento indisponível: usa o padrão.
  }
  return DEFAULT_PLAN_TARGET;
}

export function savePlanTarget(target: number) {
  try {
    localStorage.setItem(PLAN_TARGET_KEY, String(target));
  } catch {
    // Sem armazenamento, a meta vale só nesta sessão.
  }
}

export function planInstruction(target: number): string {
  return [
    "Você está no modo de planejamento. Até o usuário aprovar o plano, siga estas regras:",
    "1. Entenda o contexto com perguntas em rodadas numeradas, sempre no bloco luna-perguntas. Pergunte só o que não dá para descobrir lendo o código.",
    "2. Em toda resposta, escreva na primeira linha \"Contexto entendido: X%\", com X de 0 a 100: a parte do contexto que você já tem para montar o plano. Essa linha é obrigatória, mesmo quando há perguntas.",
    `3. Só apresente o plano quando o contexto entendido chegar a ${target}% ou mais. Abaixo disso, continue perguntando, sem escrever o plano.`,
    "4. Quando o contexto estiver claro, pesquise na web as boas práticas do que foi combinado e cite a fonte, antes de montar o plano. Diga a melhor prática em uma ou duas frases. Se não tiver ferramenta de pesquisa nesta sessão, diga isso antes de planejar. A decisão final é do usuário: se ele escolher outra opção, siga a escolha dele e registre o risco em ## Riscos.",
    "5. Não altere arquivos nem execute comandos que modifiquem algo.",
    "6. Ao apresentar o plano, escreva primeiro um resumo de no máximo 3 linhas, fora de qualquer bloco. Depois, o plano completo dentro de um bloco de código chamado luna-plano, em Markdown, com estas seções: ## Contexto; ## Arquivos a alterar (caminho e o que muda em cada um); ## Passos (numerados, cada um com a verificação); ## Não mexer (arquivos e pastas que não devem ser tocados, e o motivo); ## Testes; ## Riscos. Cite os arquivos pelo caminho, sem trechos de código.",
    "7. Termine perguntando se pode aprovar, com o bloco de perguntas e as opções \"Aprovar e executar\" e \"Ainda não\".",
    "8. Só comece a executar depois que o usuário aprovar.",
  ].join("\n");
}

const PLAN_BLOCK = /```luna-plano\s*\n([\s\S]*?)\n```/;

/// Separa o plano completo (bloco `luna-plano`) do resumo que fica no chat. Sem o bloco, devolve null.
export function extractPlan(reply: string): { summary: string; plan: string } | null {
  const match = PLAN_BLOCK.exec(reply);
  if (!match || !match[1].trim()) return null;
  return { summary: reply.replace(match[0], "").trim(), plan: match[1].trim() };
}

/// O plano que já chegou até agora, enquanto o bloco `luna-plano` ainda está sendo escrito. Sem o bloco, devolve null.
export function streamingPlan(reply: string): string | null {
  const start = reply.indexOf("```luna-plano");
  if (start < 0) return null;
  const body = reply.slice(start).replace(/^```luna-plano[ \t]*\r?\n?/, "");
  // Tira a cerca de fechamento, inteira ou parcial (ainda chegando).
  return body.replace(/\n?`{0,3}\s*$/, "").trim();
}

/// A resposta pede a aprovação do plano (a aprovação é a última pergunta do plano).
function asksApproval(reply: { questions: Question[] | null }): boolean {
  return reply.questions?.some((q) => q.options.includes(APPROVE_OPTION)) ?? false;
}

/// O texto que vira plano na aba Plano, ou null. Só vira plano a resposta com o bloco `luna-plano` ou que pede a aprovação.
/// Rodada de perguntas e conversa (ex.: um agradecimento) não viram plano: senão apagam o plano que está na aba.
export function planFromReply(reply: { text: string; questions: Question[] | null }, planning: boolean): string | null {
  if (!planning || !reply.text.trim()) return null;
  if (asksApproval(reply)) return reply.text;
  if (reply.questions) return null;
  return extractPlan(reply.text) ? reply.text : null;
}

/// O plano está pronto quando a resposta traz o bloco `luna-plano` ou pede a aprovação. Rodada de perguntas não está pronta.
export function planIsReady(reply: { text: string; questions: Question[] | null }): boolean {
  return extractPlan(reply.text) !== null || asksApproval(reply);
}

/// Lê "Contexto entendido: X%" da resposta. Sem a linha, devolve null.
const UNDERSTOOD = /Contexto entendido:\s*(\d{1,3})\s*%/i;
export function contextUnderstood(text: string): number | null {
  const match = UNDERSTOOD.exec(text);
  return match ? Math.min(100, Number(match[1])) : null;
}

/// Opção que aprova o plano quando o agente pergunta pelo seletor. Escolhê-la encerra o planejamento.
export const APPROVE_OPTION = "Aprovar e executar";

/// Enviado quando o usuário aprova: o turno seguinte já sai do modo plano.
/// Registro das etapas com as ferramentas de tarefa do Claude Code (TaskCreate/TaskUpdate).
const TASK_STEPS_CLAUDE = "Antes de editar, crie uma tarefa (TaskCreate) para cada etapa do plano, na ordem. Ao começar uma etapa, marque-a como in_progress (TaskUpdate); ao terminar, marque-a como completed. Antes de terminar, marque todas as etapas como completed, inclusive a última.";
/// Registro das etapas com o servidor de etapas do Luna (lunatasks), para o OpenCode e o Codex: eles não têm TaskCreate.
const TASK_STEPS_LUNA = "Antes de editar, registre as etapas do plano na ordem, chamando tools.lunatasks.set_steps dentro da ferramenta execute, com steps: [{ content, status }] (content é o texto curto da etapa; status é pending, in_progress ou completed). Chame de novo a cada mudança de estado, sempre com a lista inteira. Antes de terminar, marque todas as etapas como completed, inclusive a última, com set_steps.";

/// A spec segue o modelo to-spec do Matt Pocock: é o que o projeto é e as decisões tomadas, não o plano passo a passo.
export const APPROVAL_PROMPT = [
  "O plano foi aprovado. Antes de começar, crie um arquivo Markdown em spec/ na raiz do projeto (ex.: spec/<funcionalidade>.md) com a spec deste trabalho, nesta ordem de seções:",
  "## Problem Statement",
  "## Solution",
  "## User Stories",
  "## Implementation Decisions",
  "## Testing Decisions",
  "## Out of Scope",
  "## Further Notes",
  "Regras da spec: descreva decisões técnicas fixas do projeto (ex.: 'frontend em React', sem outro framework que não foi escolhido). Em Implementation Decisions, não coloque caminhos de arquivo nem trechos de código. Em Testing Decisions, só comportamento externo. Não copie o plano passo a passo.",
  TASK_STEPS_CLAUDE,
  "Depois, execute o plano agora, editando os arquivos necessários, e resuma o que foi feito ao final.",
].join("\n");

/// Onde a spec é escrita sem pasta da conversa (projetos antigos). Com pasta, a frase troca pelo caminho da spec.
const SPEC_LOCATION = "em spec/ na raiz do projeto (ex.: spec/<funcionalidade>.md)";

/// Aprovação para um motor. O Claude usa TaskCreate; OpenCode e Codex usam o servidor de etapas do Luna.
/// Com `specDir` (specs/NNN-slug), a spec vai para `<specDir>/spec.md`, e as decisões ficam registradas nessa pasta.
export function approvalInstruction(provider: ProviderKind, specDir?: string): string {
  const base = specDir ? APPROVAL_PROMPT.replace(SPEC_LOCATION, `em ${specDir}/spec.md`) : APPROVAL_PROMPT;
  if (provider === "claude") return base;
  // O Copilot e o Antigravity ainda não recebem o servidor de etapas do Luna (falta a config MCP): sem registro de etapas por enquanto.
  if (provider === "copilot" || provider === "antigravity") return base.replace(TASK_STEPS_CLAUDE, "");
  return base.replace(TASK_STEPS_CLAUDE, TASK_STEPS_LUNA);
}

/// Oferta final da execução: o agente pergunta se o usuário quer as revisões, e recomenda rodá-las.
export const REVIEW_OFFER = [
  "Quando terminar a implementação, pergunte ao usuário se ele quer rodar /code-review e /security-review. Recomende rodar as duas. Se a execução alterou arquivos, use só esses arquivos como escopo.",
  "Faça essa pergunta somente dentro do bloco luna-perguntas, com as opções \"Rodar as revisões\" e \"Não rodar\", e \"recomendada\" igual a \"Rodar as revisões\".",
].join("\n");

/// Aprovação com o plano aprovado dentro. Sem ele (plano não guardado), vale só a instrução.
export function approvalPrompt(plan: string | null, provider: ProviderKind = "claude", specDir?: string): string {
  const review = provider === "claude" ? `\n\n${REVIEW_OFFER}` : "";
  const instruction = `${approvalInstruction(provider, specDir)}${review}`;
  if (!plan?.trim()) return instruction;
  return `${instruction}\n\nPlano aprovado, para usar como referência durante toda a execução:\n\`\`\`luna-plano\n${plan}\n\`\`\``;
}

/// Protocolo das perguntas: toda pergunta vai dentro do bloco, que o app mostra como seletor.
/// Sem o bloco, a pergunta apareceria como texto no chat, o que o usuário não quer.
export const QUESTION_PROTOCOL = [
  "Sempre que precisar perguntar algo ao usuário, escreva as perguntas somente dentro de um bloco de código chamado luna-perguntas, com um array JSON:",
  "```luna-perguntas",
  '[{"pergunta": "texto da pergunta", "opcoes": ["opção A", "opção B"], "recomendada": "opção A", "multipla": false}]',
  "```",
  "Cada pergunta tem de 2 a 6 opções. Use \"multipla\": true só quando mais de uma opção puder valer.",
  "\"recomendada\" é a opção que você recomenda, copiada exatamente de \"opcoes\". Sempre informe uma.",
  "Nunca escreva perguntas fora desse bloco.",
].join("\n");

export function withPlanInstruction(prompt: string, target = DEFAULT_PLAN_TARGET): string {
  return `${planInstruction(target)}\n\n${QUESTION_PROTOCOL}\n\n${prompt}`;
}
