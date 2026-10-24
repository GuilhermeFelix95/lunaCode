# Consumo de tokens: análise e reversão da compactação

## Problem Statement

O usuário sente que o limite semanal de uso das CLIs (Claude e Codex) está sendo consumido rápido demais, mesmo depois das melhorias da versão 0.2.0. Não há uma explicação clara de onde vêm os tokens de cada turno, e a comparação com a versão 0.1.9 precisa ser feita sem gastar cota.

## Solution

- Entender, por leitura do código, o que o app envia em cada turno e o que cresceu desde a versão 0.1.9.
- Confirmar quais correções já estão na branch e quais partes ainda pesam no pedido.
- Voltar a compactação automática da conversa para o limite anterior de 90% da janela de contexto.
- Corrigir apenas o que for confirmado pela leitura, sem medir consumo real.

## User Stories

1. Como usuário, quero saber por que o limite semanal some rápido, para decidir o que ajustar.
2. Como usuário, quero que as correções de consumo já feitas sejam verificáveis, para confiar que o consumo caiu.
3. Como usuário, quero que a conversa seja compactada só quando estiver perto do limite da janela, para não perder contexto antes do necessário.

## Implementation Decisions

- A análise é só de leitura: não roda o app, não envia mensagens e não gasta cota.
- O escopo é o Claude e o Codex. O OpenCode fica fora desta análise.
- A comparação de referência é a versão 0.1.9, sem medir valores reais.
- A compactação automática volta ao limite de 90% da janela de contexto.
- O relatório separa causas confirmadas, hipóteses não confirmadas e o que ficou fora.
- Nenhuma dependência nova é adicionada.

## Testing Decisions

- A conversa com o contexto abaixo de 90% da janela não é compactada sozinha.
- A conversa com o contexto a partir de 90% da janela é compactada sozinha.
- O pedido de cada turno não repete a especificação da conversa depois do primeiro turno da sessão.
- Quando a sessão é nova, a especificação volta a entrar no pedido uma vez.

## Out of Scope

- Medir o consumo real de cada conversa ou comparar valores em cota.
- Mudar o conteúdo da identidade da Luna ou das instruções de tarefas.
- Alterar o OpenCode e os outros provedores.
- Mudar o formato das conversas salvas ou a API de plugins.
- Corrigir o teste de estilo do composer que já falha no main.
- Reescrever linhas antigas do MEMORY.md.

## Further Notes

- As correções já presentes na branch (especificação enviada uma vez por sessão, instrução de concisão e navegador carregado só quando em uso) são verificadas pela leitura, não por medição.
- O resultado da análise é registrado no relatório da conversa, e a linha de registro segue o formato `AAAA-MM-DD: o que foi feito` do MEMORY.md.
