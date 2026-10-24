/// Perguntas do agente: vêm num bloco `luna-perguntas` com um array JSON, e viram um seletor em vez de texto no chat.
/// Assim a pergunta tem as mesmas opções em qualquer motor, sem depender de ferramenta própria de cada CLI.

export interface Question {
  question: string;
  options: string[];
  multiple: boolean;
  /** Opção que o agente recomenda. Sempre está em `options`, marcada no seletor. */
  recommended?: string;
}

export interface ReplyParts {
  /** Texto da resposta sem o bloco de perguntas. */
  text: string;
  /** Perguntas válidas, ou `null` quando não há bloco ou ele está malformado. */
  questions: Question[] | null;
  /** Aviso para o usuário quando o bloco existe mas não pode ser usado. */
  warning?: string;
}

/// Um turno pode fazer perguntas no seletor quando é de planejamento, quando não tem modo (envio comum)
/// ou quando o modo declara `questions`. Modo com `questions: false` fica com a resposta como texto.
export function turnAsksQuestions(plan: boolean, mode: { id: string; questions?: boolean } | undefined): boolean {
  return plan || mode === undefined || (mode.questions ?? mode.id === "ask");
}

const BLOCK = /```luna-perguntas\s*\n([\s\S]*?)\n```/;
const UNCLOSED_BLOCK = /```luna-perguntas[\s\S]*$/;
const MAX_OPTIONS = 6;
const MALFORMED = "As perguntas vieram num formato inválido, então não foram mostradas.";

/// Separa o bloco de perguntas da explicação. Sem bloco, devolve a resposta inteira.
/// Bloco malformado (sem fechamento, JSON ou formato inválido) é sempre escondido, com aviso.
export function extractQuestions(reply: string): ReplyParts {
  const match = BLOCK.exec(reply);
  if (!match) {
    if (!UNCLOSED_BLOCK.test(reply)) return { text: reply, questions: null };
    return { text: reply.replace(UNCLOSED_BLOCK, "").trim(), questions: null, warning: MALFORMED };
  }

  const textWithoutBlock = reply.replace(BLOCK, "").trim();
  let raw: unknown;
  try {
    raw = JSON.parse(match[1]);
  } catch {
    return { text: textWithoutBlock, questions: null, warning: MALFORMED };
  }
  const questions = parseQuestions(raw);
  if (!questions) return { text: textWithoutBlock, questions: null, warning: MALFORMED };
  return { text: textWithoutBlock, questions };
}

/// Confere a forma do JSON: uma lista de perguntas, cada uma com 2 a 6 opções em texto.
/// A opção `recomendada` entra na lista se faltar: o usuário sempre a vê, marcada.
function parseQuestions(raw: unknown): Question[] | null {
  if (!Array.isArray(raw) || raw.length === 0) return null;
  const questions: Question[] = [];
  for (const item of raw) {
    if (!item || typeof item !== "object") return null;
    const { pergunta, opcoes, multipla, recomendada } = item as Record<string, unknown>;
    if (typeof pergunta !== "string" || !pergunta.trim()) return null;
    if (!Array.isArray(opcoes) || opcoes.length < 2 || opcoes.length > MAX_OPTIONS) return null;
    if (!opcoes.every((option) => typeof option === "string" && option.trim())) return null;
    const options = (opcoes as string[]).map((option) => option.trim());
    const question: Question = { question: pergunta.trim(), options, multiple: multipla === true };
    if (typeof recomendada === "string" && recomendada.trim()) {
      const recommended = recomendada.trim();
      if (!options.includes(recommended)) {
        // Lista cheia: a recomendada troca a última opção, para o limite de 6 continuar valendo.
        if (options.length === MAX_OPTIONS) options.pop();
        options.unshift(recommended);
      }
      question.recommended = recommended;
    }
    questions.push(question);
  }
  return questions;
}
