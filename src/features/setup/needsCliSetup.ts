import type { CliStatus } from "../../types";

/// A tela de instalação só abre na abertura quando nenhuma CLI está instalada.
/// Com pelo menos uma instalada, o app segue sem pedido; as demais ficam nas Configurações.
export function needsCliSetup(statuses: CliStatus[]): boolean {
  return statuses.length > 0 && statuses.every((status) => !status.installed);
}
