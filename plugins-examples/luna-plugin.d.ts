// Tipos da API de plugins do LunaHarness. Use para ter autocompletar no index.js:
// no topo do arquivo, escreva  /// <reference path="../luna-plugin.d.ts" />
// Guia completo em docs/plugins.md.

/** Modo do composer. Só `readOnly`: um modo restringe, nunca amplia permissão. */
export interface PluginMode {
  id: string;
  label: string;
  description: string;
  instruction: string;
  permission: "readOnly";
  /** `true` faz o agente perguntar antes de agir, pelo seletor de perguntas do chat. */
  questions?: boolean;
}

/** Resumo do cache de prompt da conversa (medido pelo núcleo a cada turno). */
export interface CacheSummary {
  /** Taxa da sessão, pesada pelos tokens (0 a 1). null sem leitura do provedor. */
  session: number | null;
  readTotal: number;
  freshTotal: number;
  writeTotal: number;
  last: { rate: number | null; diagnosis: "aquecendo" | "sessao-nova" | "modelo-trocado" | "prompt-mudou" | "pausa" | "desconhecido" | null; model: string } | null;
  /** Taxas dos últimos 12 turnos, do mais antigo ao mais novo. */
  history: (number | null)[];
  warm: { state: "quente"; remainingMs: number } | { state: "frio" } | { state: "sem-ttl" } | { state: "sem-dados" };
  tip: string | null;
}

/** Cache de prompt da conversa. Requer `cache`. Só leitura: o plugin não muda o cache. */
export interface PluginCacheApi {
  /** Resumo da conversa. null até o primeiro turno com medição. */
  stats(): CacheSummary | null;
  /** Aviso local no chat. Não é enviado ao agente. */
  notify(text: string): void;
}

export interface LunaPluginApi {
  /** Requer `commands`. O nome vira `/nome` no chat. Devolver texto envia esse texto como pedido. */
  registerCommand(name: string, handler: (args: string) => string | void | Promise<string | void>): void;
  /** Requer `hooks`. Recebe o texto antes do envio e pode devolver outro texto. */
  onBeforeSend(hook: (prompt: string) => string | void | Promise<string | void>): void;
  /** Requer `hooks`. Recebe o texto completo da resposta. Não altera a resposta exibida. */
  onAfterResponse(hook: (text: string) => void | Promise<void>): void;
  /** Requer `autofix`. Ao fim do turno, devolver um texto pede uma correção (até 2 por envio). Sem texto, a resposta é entregue. */
  onTurnDone(hook: (turn: {
    text: string;
    ok: boolean;
    lastCommand: { ok: boolean; output: string } | null;
    attempt: number;
  }) => string | void | Promise<string | void>): void;
  /** Requer `modes`. */
  registerMode(mode: PluginMode): void;
  /** Requer `cache`. Sem essa permissão, acessar `cache` recusa o plugin. */
  readonly cache: PluginCacheApi;
  /** Requer `ui`. Pode ler e mudar qualquer parte da tela do app. Tudo que ele monta ou injeta sai ao desligar o plugin. */
  readonly ui: LunaPluginUi;
  /** Requer `theme`. Registra um tema que o usuário escolhe em Configurações > Aparência. */
  readonly theme: LunaPluginTheme;
  /** Requer `skills`. Sem essa permissão, acessar `skills` recusa o plugin. */
  readonly skills: LunaPluginSkills;
}

/** Skill instalada, sem o conteúdo do SKILL.md. */
export interface LunaPluginSkill {
  id: string;
  name: string;
  description: string;
}

/** Permissão `skills`. Só leitura: lista nome e descrição das skills instaladas. */
export interface LunaPluginSkills {
  list(): Promise<LunaPluginSkill[]>;
}

/** Permissão `theme`. */
export interface LunaPluginTheme {
  register(theme: {
    /** Id sem barra, como `gameboy`. */
    id: string;
    name: string;
    dark: boolean;
    /** Variáveis CSS como `--bg`. O valor não pode conter `;`, `{`, `}`, `<` nem `>`. */
    tokens: Record<string, string>;
    /** CSS livre do tema, até 64 KB. */
    css?: string;
  }): void;
}

/** Permissão `ui`. */
export interface LunaPluginUi {
  /** O documento inteiro da janela. */
  root(): Document;
  /** Injeta CSS na janela. */
  addStyle(css: string): void;
  /** Monta um bloco no ponto indicado (`chat`, `composer` ou `sidebar`). `render` recebe o elemento e pode devolver uma função de limpeza. */
  mount(slot: "chat" | "composer" | "sidebar", render: (el: HTMLElement) => void | (() => void)): void;
}

/** Todo plugin exporta `activate`, chamado uma vez quando o plugin é ligado ou o app abre. */
export declare function activate(api: LunaPluginApi): void | Promise<void>;
