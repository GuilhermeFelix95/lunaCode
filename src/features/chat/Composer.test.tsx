import { act, createRef } from "react";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Composer, type ComposerHandle } from "./Composer";

const effort = (id: string, label: string, recommended = false) => ({ id, label, recommended });

const CLAUDE_MODELS = [
  {
    id: "claude-opus-5-5",
    label: "Opus 5.5",
    description: "Para trabalho complexo",
    efforts: [effort("low", "Low"), effort("medium", "Medium", true), effort("max", "Max")],
    default: false,
  },
  {
    id: "claude-sonnet-5-5",
    label: "Sonnet 5.5",
    description: "Mais eficiente",
    efforts: [effort("low", "Low"), effort("medium", "Medium", true)],
    default: false,
  },
];

/// O texto inicial entra pela ponte, como a conversa faz ao sugerir ou escolher algo.
function setup(overrides: Partial<Parameters<typeof Composer>[0]> & { draft?: string } = {}) {
  const { draft = "olá", ...rest } = overrides;
  const composerRef = createRef<ComposerHandle>();
  const props = {
    composerRef,
    onDraft: vi.fn(),
    onSend: vi.fn(),
    onStop: vi.fn(),
    running: false,
    blocked: false,
    agent: "claude",
    agents: [
      { key: "claude", label: "Claude Code", engine: "claude" as const },
      { key: "api:p1", label: "DeepSeek", engine: "claude" as const, profileId: "p1" },
    ],
    onAgent: vi.fn(),
    permission: "acceptEdits" as const,
    onPermission: vi.fn(),
    models: CLAUDE_MODELS,
    model: "",
    onModel: vi.fn(),
    effort: "",
    onEffort: vi.fn(),
    planning: false,
    onTogglePlanning: vi.fn(),
    modes: [] as { id: string; label: string; description: string; instruction: string; permission: "readOnly" }[],
    mode: "",
    onMode: vi.fn(),
    ...rest,
  };
  render(<Composer {...props} />);
  act(() => composerRef.current?.setValue(draft));
  return props;
}

describe("Composer", () => {
  it("digitar guarda o texto no próprio campo e avisa a conversa a cada tecla", () => {
    const props = setup({ draft: "" });
    const field = screen.getByLabelText("Mensagem");
    fireEvent.change(field, { target: { value: "oi" } });

    expect((field as HTMLTextAreaElement).value).toBe("oi");
    expect(props.onDraft).toHaveBeenCalledWith("oi");
  });

  it("a conversa põe texto no campo pela ponte", () => {
    setup({ draft: "/plano " });

    expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).value).toBe("/plano ");
  });

  it("Enter envia a mensagem", () => {
    const props = setup();
    fireEvent.keyDown(screen.getByLabelText("Mensagem"), { key: "Enter" });

    expect(props.onSend).toHaveBeenCalledOnce();
  });

  it("Shift+Enter não envia (quebra de linha)", () => {
    const props = setup();
    fireEvent.keyDown(screen.getByLabelText("Mensagem"), { key: "Enter", shiftKey: true });

    expect(props.onSend).not.toHaveBeenCalled();
  });

  it("Enter não envia texto vazio", () => {
    const props = setup({ draft: "   " });
    fireEvent.keyDown(screen.getByLabelText("Mensagem"), { key: "Enter" });

    expect(props.onSend).not.toHaveBeenCalled();
  });

  it("durante a execução o botão vira Parar", () => {
    const props = setup({ running: true });
    fireEvent.click(screen.getByLabelText("Parar"));

    expect(props.onStop).toHaveBeenCalledOnce();
  });

  it("bloqueado (sem login) não aceita envio", () => {
    const props = setup({ blocked: true });
    fireEvent.keyDown(screen.getByLabelText("Mensagem"), { key: "Enter" });

    expect((screen.getByLabelText("Mensagem") as HTMLTextAreaElement).disabled).toBe(true);
    expect(props.onSend).not.toHaveBeenCalled();
  });

  it("o provedor pode ser trocado no meio da conversa, só não durante uma resposta", () => {
    setup();
    expect((screen.getByRole("button", { name: "Provedor" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("o seletor lista os CLIs e os perfis de API, e avisa a escolha", () => {
    const onAgent = vi.fn();
    setup({ onAgent });

    fireEvent.click(screen.getByRole("button", { name: "Provedor" }));
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Claude Code",
      "DeepSeek",
    ]);
    fireEvent.click(screen.getByRole("option", { name: "DeepSeek" }));
    expect(onAgent).toHaveBeenCalledWith("api:p1");
  });

  it("o seletor de esforço fica desligado enquanto nenhum modelo está escolhido", () => {
    setup({ model: "" });

    expect((screen.getByRole("button", { name: "Esforço" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("mostra só os níveis que o modelo escolhido aceita", () => {
    setup({ model: "claude-sonnet-5-5" });
    fireEvent.click(screen.getByRole("button", { name: "Esforço" }));

    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Padrão", "Low", "Medium (recomendado)"]);
  });

  it("marca o nível recomendado com o nome que o Desktop usa", () => {
    setup({ model: "claude-opus-5-5" });
    fireEvent.click(screen.getByRole("button", { name: "Esforço" }));

    expect(screen.getByRole("option", { name: "Medium (recomendado)" })).toBeTruthy();
  });

  it("escolher a permissão avisa o novo modo", () => {
    const props = setup();
    fireEvent.click(screen.getByRole("button", { name: "Permissão" }));
    fireEvent.click(screen.getByRole("option", { name: "Só leitura" }));

    expect(props.onPermission).toHaveBeenCalledWith("readOnly");
  });

  it("o seletor de modelo mostra o nome e a descrição de cada modelo", () => {
    setup();
    fireEvent.click(screen.getByLabelText("Modelo"));

    expect(screen.getByRole("option", { name: /Opus 5\.5/ })).toBeTruthy();
    expect(screen.getByText("Para trabalho complexo")).toBeTruthy();
    expect(screen.getByRole("option", { name: /Sonnet 5\.5/ })).toBeTruthy();
  });

  it("o seletor de modelo lista só os modelos suportados, sem 'Padrão da CLI'", () => {
    setup({ model: "claude-opus-5-5" });
    fireEvent.click(screen.getByLabelText("Modelo"));

    expect(screen.queryByText("Padrão da CLI")).toBeNull();
    const list = screen.getByRole("listbox", { name: "Modelos" });
    expect(within(list).getAllByRole("option")).toHaveLength(2);
  });

  it("escolher um modelo manda o ID da CLI, não o nome de exibição", () => {
    const props = setup();
    fireEvent.click(screen.getByLabelText("Modelo"));
    fireEvent.click(screen.getByRole("option", { name: /Opus 5\.5/ }));

    expect(props.onModel).toHaveBeenCalledWith("claude-opus-5-5");
  });

  it("o botão Planejar reflete o modo e alterna ao clicar", () => {
    const props = setup({ planning: true });
    const plan = screen.getByLabelText("Modo de planejamento");

    expect(plan.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(plan);
    expect(props.onTogglePlanning).toHaveBeenCalledOnce();
  });
});

describe("Composer: modos de plugin", () => {
  const ASK = { id: "ask", label: "Conversar", description: "Só explica", instruction: "Só explique.", permission: "readOnly" as const };

  it("liga um modo ao clicar no chip", () => {
    const props = setup({ modes: [ASK] });
    fireEvent.click(screen.getByRole("button", { name: "Conversar" }));

    expect(props.onMode).toHaveBeenCalledWith("ask");
  });

  it("desliga o modo ativo ao clicar de novo", () => {
    const props = setup({ modes: [ASK], mode: "ask" });
    const chip = screen.getByRole("button", { name: "Conversar" });
    expect(chip.getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(chip);
    expect(props.onMode).toHaveBeenCalledWith("");
  });
});

describe("Composer: @arquivo e imagens", () => {
  it("escolher uma sugestão devolve o caminho do arquivo", () => {
    const props = setup({ files: ["src/Parser.ts"], onPickFile: vi.fn() });
    fireEvent.click(screen.getByRole("option", { name: "src/Parser.ts" }));

    expect(props.onPickFile).toHaveBeenCalledWith("src/Parser.ts");
  });

  it("o botão Anexar abre o seletor e entrega cada arquivo escolhido ao onAttach", () => {
    const props = setup({ onAttach: vi.fn() });
    const input = document.querySelector<HTMLInputElement>('input[type="file"]');
    const notas = new File(["# notas"], "notas.md", { type: "text/markdown" });

    expect(screen.getByRole("button", { name: "Anexar arquivo" })).toBeTruthy();
    fireEvent.change(input!, { target: { files: [notas] } });

    expect(props.onAttach).toHaveBeenCalledWith(notas);
  });

  it("envia só com anexo, sem texto", () => {
    const props = setup({ draft: "", attachments: [{ id: 1, url: "blob:foto" }] });
    fireEvent.keyDown(screen.getByLabelText("Mensagem"), { key: "Enter" });

    expect(props.onSend).toHaveBeenCalled();
  });

  it("arquivo anexado aparece na lista e tirar devolve o id dele", () => {
    const props = setup({ fileAttachments: [{ id: 2, name: "notas.md" }], onRemoveFileAttachment: vi.fn() });

    expect(screen.getByText("notas.md")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Tirar arquivo" }));

    expect(props.onRemoveFileAttachment).toHaveBeenCalledWith(2);
  });

  it("tirar uma imagem anexada devolve o id dela", () => {
    const props = setup({ attachments: [{ id: 3, url: "blob:foto" }], onRemoveAttachment: vi.fn() });
    fireEvent.click(screen.getByRole("button", { name: "Tirar imagem" }));

    expect(props.onRemoveAttachment).toHaveBeenCalledWith(3);
  });

  it("menu de / mostra os itens com a origem e Enter escolhe o destacado, sem enviar", () => {
    const slash = [
      { name: "plano", description: "Liga o planejamento", origin: "luna" as const },
      { name: "pdf", description: "Lê PDFs", origin: "skill" as const },
    ];
    const props = setup({ draft: "/p", slash, onPickSlash: vi.fn() });
    const list = screen.getByRole("listbox", { name: "Comandos e skills" });

    expect(within(list).getAllByRole("option")).toHaveLength(2);
    expect(within(list).getByText("Skill")).toBeTruthy();

    fireEvent.keyDown(screen.getByLabelText("Mensagem"), { key: "ArrowDown" });
    fireEvent.keyDown(screen.getByLabelText("Mensagem"), { key: "Enter" });

    expect(props.onPickSlash).toHaveBeenCalledWith(slash[1]);
    expect(props.onSend).not.toHaveBeenCalled();
  });

  it("comando digitado por completo, sem espaço, é enviado com Enter (não vira só a escolha do menu)", () => {
    const slash = [
      { name: "plano", description: "Liga o planejamento", origin: "luna" as const },
      { name: "planejar-depois", description: "Outro", origin: "skill" as const },
    ];
    const props = setup({ draft: "/plano", slash, onPickSlash: vi.fn() });

    fireEvent.keyDown(screen.getByLabelText("Mensagem"), { key: "Enter" });

    expect(props.onSend).toHaveBeenCalledTimes(1);
    expect(props.onPickSlash).not.toHaveBeenCalled();
  });

  it("sem contas no motor, o seletor de conta não aparece", () => {
    setup();
    expect(screen.queryByRole("button", { name: "Conta" })).toBeNull();
  });

  it("escolher uma conta avisa o pai com o id dela", () => {
    const accounts = [{ id: "claude-1", provider: "claude" as const, name: "Trabalho" }];
    const props = setup({ accountsEnabled: true, accounts, account: "", onAccount: vi.fn() });

    fireEvent.click(screen.getByRole("button", { name: "Conta" }));
    fireEvent.click(screen.getByRole("option", { name: "Trabalho" }));

    expect(props.onAccount).toHaveBeenCalledWith("claude-1");
  });

});

describe("Composer: modo /meta", () => {
  it("o botão /meta reflete o modo ligado e alterna ao clicar", () => {
    const props = setup({ goalOn: true, onToggleGoal: vi.fn() });
    const goal = screen.getByLabelText("Modo /meta");

    expect(goal.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(goal);
    expect(props.onToggleGoal).toHaveBeenCalledOnce();
  });

  it("o botão mostra o nome Meta, como o Planejar, e não só um ícone", () => {
    setup({ onToggleGoal: vi.fn() });
    expect(screen.getByRole("button", { name: "Modo /meta" }).textContent).toBe("Meta");
  });

  it("durante uma resposta com a meta ativa, o botão segue habilitado para desligar a meta", () => {
    const props = setup({ running: true, goalOn: true, goalActive: true, onToggleGoal: vi.fn() });
    const goal = screen.getByRole("button", { name: "Modo /meta" });

    expect((goal as HTMLButtonElement).disabled).toBe(false);
    expect(goal.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(goal);
    expect(props.onToggleGoal).toHaveBeenCalledOnce();
  });

  it("durante uma resposta sem meta ativa, o botão fica desabilitado", () => {
    setup({ running: true, onToggleGoal: vi.fn() });
    expect((screen.getByRole("button", { name: "Modo /meta" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("Composer: velocidade da resposta", () => {
  it("mostra a velocidade medida durante a resposta, sem botão para ligar", () => {
    setup({ running: true, speed: 42 });
    expect(screen.getByText("42 tokens/s")).toBeTruthy();
    expect(screen.queryByLabelText("Mostrar velocidade da resposta")).toBeNull();
  });

  it("depois do turno, continua mostrando o valor final medido", () => {
    setup({ running: false, speed: 42 });
    expect(screen.getByText("42 tokens/s")).toBeTruthy();
  });

  it("sem medição ainda, não mostra número nenhum", () => {
    setup({ running: true, speed: null });
    expect(screen.queryByText(/tokens\/s/)).toBeNull();
  });
});

describe("botão Clonar site", () => {
  it("fica ao lado de Meta", () => {
    setup({ onClone: vi.fn() });

    const meta = screen.getByRole("button", { name: "Modo /meta" });
    const clone = screen.getByRole("button", { name: "Clonar um site para o projeto (beta)" });
    expect(meta.nextElementSibling).toBe(clone);
  });

  it("sem o callback, o botão não aparece", () => {
    setup();

    expect(screen.queryByRole("button", { name: "Clonar um site para o projeto (beta)" })).toBeNull();
  });

  it("chama o callback do fluxo ao clicar", () => {
    const onClone = vi.fn();
    setup({ onClone });
    fireEvent.click(screen.getByRole("button", { name: "Clonar um site para o projeto (beta)" }));

    expect(onClone).toHaveBeenCalledOnce();
  });

  it("não aparece no modo navegador, como Meta", () => {
    setup({ browserMode: true, onClone: vi.fn() });

    expect(screen.queryByRole("button", { name: "Clonar um site para o projeto (beta)" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Meta" })).toBeNull();
  });

  it("no modo navegador some IA, modelo, esforço e conta, mas o envio continua", () => {
    setup({ browserMode: true, accountsEnabled: true });

    expect(screen.queryByRole("button", { name: "Provedor" })).toBeNull();
    expect(screen.queryByLabelText("Modelo")).toBeNull();
    expect(screen.queryByRole("button", { name: "Conta" })).toBeNull();
    expect(screen.queryByLabelText("Esforço")).toBeNull();
    expect(screen.getByRole("button", { name: "Anexar arquivo" })).toBeTruthy();
  });

  it("fica indisponível com o motivo quando não há pasta de projeto", () => {
    const onClone = vi.fn();
    setup({ onClone, cloneDisabledReason: "Abra uma pasta de projeto para clonar um site." });
    const button = screen.getByRole("button", { name: "Clonar um site para o projeto (beta)" }) as HTMLButtonElement;

    expect(button.disabled).toBe(true);
    expect(button.title).toBe("Abra uma pasta de projeto para clonar um site.");
    fireEvent.click(button);
    expect(onClone).not.toHaveBeenCalled();
  });
});
