/// Compactação da conversa: a Luna pede um resumo ao agente atual e, depois, começa uma sessão nova com ele.
import type { AgentSessions } from "../types";
/// Vale igual para Claude, Codex e perfis de API, porque o resumo vai no texto do pedido, não na CLI.

/// Pedido do resumo. Não altera arquivos: é só leitura da conversa.
export const COMPACT_PROMPT = [
  "Resuma esta conversa para que ela possa continuar numa sessão nova, sem perder o que importa.",
  "Responda só com o resumo, em Markdown, com estas seções:",
  "## Objetivo",
  "## Decisões",
  "## Arquivos tocados (caminho e o que mudou)",
  "## Estado atual",
  "## Próximos passos",
  "## Pendências",
  "Não altere nenhum arquivo nesta resposta.",
].join("\n");

/// Limite de uso do contexto a partir do qual a conversa é compactada sozinha.
export const AUTO_COMPACT_RATIO = 0.9;

/// Início da sessão nova: o resumo que a conversa anterior deixou.
export function compactPreamble(summary: string): string {
  return [
    "Esta conversa foi compactada. Este é o resumo do que já aconteceu até aqui, escrito pelo agente anterior:",
    summary.trim(),
    "Use o resumo como o contexto da conversa. Releia os arquivos antes de editá-los.",
  ].join("\n\n");
}

/// Se o contexto passou do limite, a conversa deve ser compactada antes do próximo envio.
export function needsCompaction(tokens: number, windowSize: number | undefined): boolean {
  return windowSize !== undefined && windowSize > 0 && tokens / windowSize >= AUTO_COMPACT_RATIO;
}

export const AUTO_COMPACT_KEY = "luna.autoCompact";

/// Compactação automática vale, salvo quando o usuário desligou em Configurações. Padrão: ligada.
export function autoCompactEnabled(): boolean {
  try {
    return localStorage.getItem(AUTO_COMPACT_KEY) !== "0";
  } catch {
    return true;
  }
}

export function setAutoCompact(on: boolean) {
  try {
    localStorage.setItem(AUTO_COMPACT_KEY, on ? "1" : "0");
  } catch {
    // Sem armazenamento, a escolha vale só nesta sessão.
  }
}

/// Sessões criadas depois do corte, com `seen` contado a partir dele. As anteriores são descartadas:
/// retomá-las traria de volta o contexto que a compactação quer deixar para trás.
export function sessionsFrom(sessions: AgentSessions, cut: number): AgentSessions {
  const kept: AgentSessions = {};
  for (const [key, session] of Object.entries(sessions)) {
    if (session && session.seen >= cut) kept[key] = { ...session, seen: session.seen - cut };
  }
  return kept;
}
