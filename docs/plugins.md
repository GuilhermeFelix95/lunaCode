# Guia para criar plugins do LunaHarness

Este guia vale para a Luna (o agente) e para quem usa o app. Ele vem embutido no LunaHarness e é gravado a cada abertura numa pasta de dados. Ao criar um plugin, leia este arquivo inteiro antes de escrever código.

## Para a Luna: leia primeiro

**O que é o LunaHarness.** É um app desktop que controla as CLIs do Claude Code e do Codex. O usuário conversa com o agente numa janela de chat, com seletores de modelo, permissão e projeto. Um plugin acrescenta comportamento a esse chat. Ele não controla o computador.

**Onde criar.** Um plugin é uma pasta chamada `<id>` dentro da pasta de rascunhos `plugin-drafts`. Ela fica ao lado da pasta `docs` onde este guia está gravado: se o caminho deste arquivo for `...\docs\plugins.md`, o rascunho vai em `...\plugin-drafts\<id>\`. O caminho da pasta de rascunhos também vem na instrução do sistema, como "Pasta de rascunhos de plugin". Crie plugins só nessa pasta, a menos que o usuário peça outra.

**O que entregar.** Dois arquivos, e só esses:
- `manifest.json`: metadados e permissões.
- `index.js`: um módulo ES de um arquivo só, que exporta `activate(api)`.

Não use TypeScript, não use `import` de outros arquivos nem de URLs, não use pacotes npm e não crie subpastas de código. O módulo é carregado como um blob, então qualquer `import` relativo falha.

**Depois de criar.** Não copie o plugin para outra pasta. Termine a resposta com um bloco de código chamado `luna-plugin`, com o caminho absoluto da pasta do rascunho:

````
```luna-plugin
{"pasta": "<caminho absoluto da pasta do plugin>"}
```
````

O app mostra um cartão com o botão **Instalar**, que abre a tela de permissões. Diga ao usuário que é só clicar em **Instalar**, que o plugin vale no próximo envio e que pode ser desligado em **Configurações > Plugins**.

**Se o pedido estiver fora da API.** Diga isso com clareza e proponha o que é possível. Não invente funções, módulos ou permissões. A lista exata de chamadas está na seção "API" abaixo.

**Como conferir o que escreveu.** Confirme que o `index.js` contém `export function activate(api)`, que cada chamada está na seção "API" e que o `manifest.json` é JSON válido. Se houver Node.js, copie o `index.js` para um arquivo `.mjs` temporário fora da pasta do plugin e rode `node --check` nele.

## Manifesto (`manifest.json`)

```json
{
  "id": "resumir-conversa",
  "name": "Resumir conversa",
  "version": "0.1.0",
  "entry": "index.js",
  "permissions": ["commands"]
}
```

| Campo | Regra |
|---|---|
| `id` | Minúsculas, números e hífen. Não pode começar com hífen. Máximo de 64 caracteres. Tem de ser igual ao nome da pasta. |
| `name` | Obrigatório, não pode ser vazio. |
| `version` | Texto livre; recomenda-se semver (`0.1.0`). |
| `entry` | Caminho relativo dentro da pasta. Não pode sair dela (`../` é recusado). Use `index.js`. |
| `permissions` | Lista com `commands`, `hooks`, `modes` e/ou `cache`. Qualquer outro valor recusa o plugin. |

Um plugin com manifesto inválido aparece na lista de avisos e não impede os outros de carregar. O arquivo de entrada precisa caber em 512 KB.

## Módulo (`index.js`)

```js
export function activate(api) {
  api.registerCommand("resumir", (args) => `Resuma a conversa em três tópicos. ${args}`.trim());
  api.onBeforeSend((prompt) => `Responda em português do Brasil.\n\n${prompt}`);
}
```

- `activate(api)` é chamado quando o plugin é carregado: ao abrir o app, ao ligar o plugin ou ao mudar a lista de plugins. Pode ser `async`.
- Se `activate` lançar erro, tudo que o plugin já registrou é desfeito. O erro aparece nos avisos do chat, com o nome do plugin.
- Um plugin não enxerga o estado de outros plugins nem da conversa, a não ser pelo que recebe nos handlers.

## API

Só estas chamadas existem. Qualquer outra é erro.

### `api.registerCommand(nome, handler)`: permissão `commands`

Registra `/nome` no chat. O `nome` começa com letra minúscula e usa só minúsculas, números e hífen. Não pode repetir um comando já existente, nem de outro plugin.

O `handler(args)` recebe o texto digitado depois do comando (uma string, talvez vazia). Ele pode ser `async`.

- Se devolver **texto**, esse texto vai ao agente como pedido. É o uso principal: o comando vira um atalho para uma instrução.
- Se devolver **nada**, o comando só roda o código local. Não há resposta visível para o usuário, então prefira devolver texto.

### `api.onBeforeSend(hook)`: permissão `hooks`

`hook(prompt)` roda antes de cada envio, na ordem em que os plugins carregaram. Se devolver texto, esse texto substitui o pedido. Se devolver nada, o pedido segue igual. Comandos também passam por esse hook, sobre o texto que o comando gerou.

### `api.onAfterResponse(hook)`: permissão `hooks`

`hook(text)` recebe o texto completo da resposta quando ela termina. Serve para registrar, contar ou reagir. Não altera a resposta que aparece no chat.

### `api.onTurnDone(hook)`: permissão `autofix`

`hook({ text, ok, lastCommand, attempt })` roda quando o turno termina, antes de a resposta sair como entregue (aviso, som). `lastCommand` é o último comando Bash do agente (`{ ok, output }`, com o fim da saída) ou `null`. Devolver um texto pede ao agente uma correção, que vira o próximo envio na mesma conversa. Até `AUTOFIX_MAX_ATTEMPTS` (2) correções por envio do usuário; depois disso a resposta sai com um aviso. Falha ou demora do hook (10 s) não segura a entrega. O LunaFix, plugin oficial e ativo por padrão, é o exemplo de referência (`plugins-examples/lunafix`).

### `api.registerMode(modo)`: permissão `modes`

Oferece um modo no composer, como um chip ao lado de "Planejar". O modo é um objeto:

```js
api.registerMode({
  id: "so-explicar",
  label: "Só explicar",
  description: "Explica, sem editar arquivos",
  instruction: "Responda só com explicações. Não edite arquivos.",
  permission: "readOnly",
  questions: false,
});
```

- `permission` **precisa** ser `"readOnly"`. Um modo só restringe; nunca dá mais permissão do que o composer já tem.
- Ligado, o modo roda o envio só com leitura e põe `instruction` na frente do pedido.
- `questions: true` faz o agente perguntar antes de agir, pelo seletor de perguntas. Sem o campo, o modo não pergunta. (O id `ask` é exceção por compatibilidade com a versão antiga do plugin oficial.)
- Só um modo fica ligado por vez: ligar um desliga o anterior.
- `id` e `label` são obrigatórios; `id` segue a regra dos comandos (começa com letra), e não pode repetir um modo já existente.

### `api.cache`: permissão `cache`

Dá acesso só de leitura ao cache de prompt da conversa. O cache de prompt é o do provedor: a Anthropic e a OpenAI guardam o começo do pedido e cobram menos pelos tokens que reaproveitam. O núcleo mede isso a cada turno. O plugin não liga, desliga nem limpa nada.

Acessar `api.cache` sem a permissão `cache` recusa o plugin. O objeto tem duas operações:

- `api.cache.stats()`: devolve o resumo da conversa (`CacheSummary` em `luna-plugin.d.ts`), ou `null` antes do primeiro turno medido. Traz a taxa da sessão (`session`), os tokens lidos, novos e gravados (`readTotal`, `freshTotal`, `writeTotal`), o último turno com o motivo de um acerto baixo (`last.diagnosis`), o histórico dos últimos 12 turnos, o estado do cache agora (`warm`) e uma dica (`tip`).
- `api.cache.notify(texto)`: mostra um aviso local no chat. O texto **não** vai ao agente.

O resumo atualiza ao fim de cada turno, antes dos ganchos `onAfterResponse`. Cada conversa tem o próprio resumo; trocar de pasta zera o da conversa.

O estado `warm` depende do provedor: no Claude, `quente` com o tempo que falta para o cache expirar (5 min, ou 1 h com o cache longo ligado em Configurações > Conversa), e `frio` depois disso. No Codex o app não recebe o tempo de vida, então é `sem-ttl`. Sem leitura do cache no turno, é `sem-dados`. Nunca aparece 0% por falta de dado.

A taxa é `lidos / (lidos + gravados + novos)`, em tokens de entrada. Ela mede o que o provedor reaproveitou, não o dinheiro economizado.

### `api.ui`: permissão `ui`

Permite mexer em qualquer parte da tela do app: o chat, a barra lateral, o composer e os demais blocos. **É a permissão mais forte.** Na instalação, a Luna mostra que o plugin "pode ler e mudar qualquer parte da tela do app, inclusive o chat e a barra". Instale só plugins de quem você confia.

- `api.ui.root()`: devolve o `document` inteiro. Use com cuidado: qualquer elemento pode ser lido ou alterado.
- `api.ui.addStyle(css)`: injeta CSS. A folha sai quando o plugin é desligado.
- `api.ui.mount(slot, render)`: monta um bloco num ponto fixo da tela. `render(el)` recebe um `div` vazio e pode devolver uma função de limpeza. Os pontos são `"chat"` (topo do chat), `"composer"` (acima da caixa de texto) e `"sidebar"` (acima do rodapé da barra lateral). Se o ponto ainda não estiver na tela, o bloco entra quando ela aparecer.

Tudo que o plugin monta ou injeta é desfeito ao desligá-lo ou recarregar os plugins.

Exemplo: um aviso no topo do chat.

```js
export function activate(api) {
  api.ui.addStyle(".meu-aviso{padding:6px 10px;border-radius:var(--chip-radius);background:var(--bg-sidebar)}");
  api.ui.mount("chat", (el) => {
    el.innerHTML = '<p class="meu-aviso">Revise o plano antes de aprovar.</p>';
  });
}
```

Ainda não há ponto no cabeçalho nem na aba do painel direito. Cada ponto novo é um `data-plugin-slot` adicionado ao componente.

### `api.theme`: permissão `theme`

Registra um tema de cores e estilo que o usuário escolhe em **Configurações > Aparência**, em "Temas de plugins". Escolhido, o tema vale para o app inteiro, e o tema do app volta quando o usuário escolhe um tema do app de novo.

```js
export function activate(api) {
  api.theme.register({
    id: "gameboy",
    name: "Game Boy",
    dark: false,
    tokens: { "--bg": "#9bbc0f", "--fg": "#0f380f", "--border": "#306230" },
    css: "* { border-radius: 0 !important; } body { font-family: monospace !important; }",
  });
}
```

- `id`: letras minúsculas, números e hífen, sem barra. O tema vira `plugin:<plugin>/<id>`.
- `tokens`: variáveis CSS do próprio app, como `--bg`, `--fg`, `--surface`, `--border` e `--accent`. O valor não pode conter `;`, `{`, `}`, `<` nem `>`, e tem no máximo 200 caracteres.
- `css`: CSS livre, aplicado enquanto o tema estiver escolhido. Até 64 KB.
- O tema some quando o plugin é desligado ou recarregado. A escolha salva volta a valer quando o plugin carrega de novo.
- Exemplo completo: `plugins-examples/gameboy-theme/`.

Assets: coloque os arquivos em `assets/` dentro da pasta do plugin e referencie-os com `asset(assets/arquivo.ext)` num valor de token ou no CSS. O app troca a referência por uma URL local do arquivo. Tipos aceitos: png, jpg, webp, gif, svg, woff2, ttf, ogg, mp3 e wav, com até 5 MB por arquivo. Caminhos com `..`, barra invertida ou fora de `assets/` são recusados.

```js
api.theme.register({
  id: "pixel",
  name: "Pixel",
  dark: false,
  tokens: { "--app-bg-image": "asset(assets/fundo.png)" },
  css: "@font-face { font-family: Pixel; src: asset(assets/pixel.woff2); } body { font-family: Pixel; }",
});
```

Mascote e som de resposta também usam assets. `mascot` troca o átomo da Luna pela imagem enquanto o tema estiver escolhido; `sounds.reply` troca o som de resposta por um arquivo do plugin (desligar o som no app continua desligando tudo).

```js
api.theme.register({
  id: "pixel",
  name: "Pixel",
  dark: false,
  tokens: {},
  mascot: "asset(assets/luna.png)",
  sounds: { reply: "asset(assets/ding.ogg)" },
});
```

## Limites

- Comandos e hooks têm **10 segundos**. Passou disso, o resultado é ignorado, o erro vai para os avisos e o envio segue sem ele.
- Um erro num handler aparece nos avisos com o nome do plugin; os outros plugins continuam.
- Plugin desligado em **Configurações > Plugins** deixa de valer na hora, até no meio de uma resposta. O código dele nem é lido do disco.

## O que a API NÃO faz (hoje)

Esta lista é importante para não prometer o que não existe:

- não lê nem grava arquivos do computador ou do projeto;
- não executa comandos do sistema;
- não faz requisições de rede;
- não cria painéis próprios nem janelas: só mexe nos pontos da tela e no que já existe, e isso exige a permissão `ui`;
- não lê nem envia chaves de API;
- um hook não consegue alterar a resposta exibida, só o texto enviado.

Quando o pedido precisar de uma dessas coisas, a resposta certa é dizer que a API não tem esse recurso ainda, e descrever o que o plugin consegue fazer. Plugins que precisam desses recursos dependem de uma nova versão da API, com permissões próprias e isolamento.

## Receitas

**Atalho que pede resposta em tópicos**

```js
export function activate(api) {
  api.registerCommand("topicos", (args) => `Responda em tópicos curtos: ${args}`.trim());
}
```

**Responder sempre em português**

```js
export function activate(api) {
  api.onBeforeSend((prompt) => `Responda em português do Brasil.\n\n${prompt}`);
}
```

**Modo que só explica**

```js
export function activate(api) {
  api.registerMode({
    id: "explicar",
    label: "Explicar",
    description: "Só explica, sem editar arquivos",
    instruction: "Responda só com explicações. Não edite arquivos.",
    permission: "readOnly",
    questions: false,
  });
}
```

**Registrar o tamanho da última resposta**

```js
export function activate(api) {
  api.onAfterResponse((text) => {
    console.info(`Luna: resposta com ${text.length} caracteres`);
  });
}
```

## Compartilhar um plugin

Hoje um plugin é só uma pasta com `manifest.json` e `index.js`. Para enviar a alguém:

1. Compacte a pasta do plugin em um ZIP e envie.
2. Quem recebe extrai o ZIP e usa **Configurações > Plugins > Instalar de uma pasta**. Esse botão é só para conteúdo compartilhado por outra pessoa; um plugin criado pela Luna se instala pelo cartão no chat.

Antes de instalar um plugin de outra pessoa, leia o `index.js`. Plugins rodam na mesma janela do app e não há sandbox ainda: a lista de permissões controla a API oferecida, não o que o código faz.

**Publicar para muitos usuários.** Um catálogo é um arquivo `marketplace.json` hospedado em `https`. O usuário cola o endereço em **Configurações > Plugins > Marketplace**.

```json
{
  "name": "Meu catálogo",
  "plugins": [
    {
      "id": "topicos",
      "name": "Tópicos",
      "description": "Atalho /topicos",
      "author": "Seu nome",
      "version": "0.1.0",
      "entry": "index.js",
      "permissions": ["commands"],
      "files": [
        { "path": "index.js", "sha256": "<hash do conteúdo>", "content": "export function activate(api) { /* ... */ }" }
      ]
    }
  ]
}
```

- `sha256` é o hash hexadecimal do conteúdo exato do arquivo. No Windows: `Get-FileHash -Algorithm SHA256 index.js`. No Linux ou macOS: `sha256sum index.js`.
- Cada arquivo tem `content` (embutido) **ou** `url` (https), não os dois.
- Um hash diferente recusa a instalação inteira.
- O `manifest.json` não entra em `files`: o app gera o manifesto a partir dos campos do catálogo.
- Ids repetidos recusam o catálogo inteiro.
- O hash confere o conteúdo, mas não prova quem publicou. Assinatura de catálogo ainda não existe.

**Pelo terminal.** `luna plugin add <id>` instala um plugin do catálogo oficial (`src-tauri/marketplace/official.json`). O comando confere os hashes, mostra as permissões e pede confirmação `(s/N)` antes de gravar em `<pasta de dados do app>/plugins/<id>`. Reinicie o app para ver o plugin. Skills e MCP têm comandos próprios (`luna skills add` e `luna mcp add`), descritos em `docs/skills.md` e `docs/mcp.md`.

## Boas práticas

- Use nomes de comando em minúsculas, sem espaço.
- Devolva instruções claras para o agente, no idioma em que o usuário conversa.
- Não coloque chaves de API, senhas ou tokens no plugin.
- Mantenha os handlers rápidos: o limite de 10 segundos vale para cada um.
- Um plugin faz uma coisa bem feita. Prefira dois plugins pequenos a um grande.

## Erros comuns

- Campo obrigatório faltando no manifesto, ou `permissions` com valor fora da lista.
- `entry` apontando para fora da pasta (`../`).
- `import` de outro arquivo: o módulo é um arquivo só.
- `id` com letras maiúsculas ou espaços.
- `registerMode` com `permission` diferente de `"readOnly"`.
- Chamar uma função que a API não tem, como `api.readFile`. Ela não existe.

## Fora da API (planejado)

- Permissões de sistema de arquivos e rede, com isolamento real (sandbox).
- Painel próprio na aba do painel direito (`registerPanel`) e pontos de interface além de chat, composer e barra.
- Plugins com vários arquivos e TypeScript.
- Assets de tema (imagens, fontes, sons) e mascote por estado.
- Assinatura de catálogos.
- Atualização automática de plugins instalados.
