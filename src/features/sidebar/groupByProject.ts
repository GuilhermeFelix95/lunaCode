import type { ConversationSummary } from "../../types";
import { groupByDay, type ConversationGroup } from "./groupByDay";

export interface ProjectGroup {
  /** Pasta completa: identifica o projeto. */
  cwd: string;
  /** Último nome da pasta, para mostrar na barra lateral. */
  name: string;
  /** Conversas do projeto, separadas por dia. */
  days: ConversationGroup[];
}

/// Agrupa conversas por pasta de trabalho. Projetos vêm em ordem alfabética pelo nome da pasta.
export function groupByProject(items: ConversationSummary[], now: Date): ProjectGroup[] {
  const byCwd = new Map<string, ConversationSummary[]>();
  for (const item of items) {
    const list = byCwd.get(item.cwd) ?? [];
    list.push(item);
    byCwd.set(item.cwd, list);
  }

  return [...byCwd.entries()]
    .sort(([cwdA], [cwdB]) => compareProjects(cwdA, cwdB))
    .map(([cwd, list]) => ({
      cwd,
      name: folderName(cwd),
      days: groupByDay([...list].sort((a, b) => b.updatedAt - a.updatedAt), now),
    }));
}

/// Ordena pelo nome da pasta ignorando maiúsculas e acentos; nomes iguais desempatam pelo caminho.
/// Pela mesma regra, `Álbum` vem antes de `Alfa`, porque a comparação segue as letras base.
function compareProjects(cwdA: string, cwdB: string): number {
  const byName = folderName(cwdA).localeCompare(folderName(cwdB), undefined, { sensitivity: "base" });
  return byName !== 0 ? byName : cwdA.localeCompare(cwdB, undefined, { sensitivity: "base" });
}

/// Nome curto de uma pasta: o último segmento do caminho (aceita `\` e `/`).
export function folderName(cwd: string): string {
  const parts = cwd.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? cwd;
}
