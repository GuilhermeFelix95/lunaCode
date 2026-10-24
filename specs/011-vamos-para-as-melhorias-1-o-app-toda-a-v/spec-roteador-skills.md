# Roteador de skills por pedido

## Problem Statement

O Claude carrega a lista de todas as skills instaladas em cada turno, e a conversa é relida a cada mensagem. Como o usuário tem muitas skills, esse prefixo fixo fica grande mesmo quando o pedido não precisa de nenhuma. O usuário quer que o Luna sugira ao agente só as skills que batem com o pedido.

## Solution

- Um plugin do LunaHarness olha o pedido e escolhe, pelo nome e pela descrição, as skills que batem com ele.
- Ele acrescenta ao pedido uma dica com essas skills, sem alterar o texto original do usuário.
- A lista de skills vem do núcleo do app, porque a API de plugins não lê arquivos.
- A dica é só uma sugestão: o CLI continua carregando a lista de skills instalada.

## User Stories

1. Como usuário, quero que o agente receba só as skills que batem com o meu pedido, para gastar menos tokens em cada mensagem.
2. Como usuário, quero que o meu pedido chegue ao agente exatamente como escrevi, para que a resposta não mude por causa da dica.
3. Como desenvolvedor de plugins, quero uma forma oficial de ler a lista de skills com permissão explícita, para não depender de acesso a arquivos.
4. Como usuário, quero poder desligar o plugin, para voltar ao comportamento padrão quando quiser.

## Implementation Decisions

- A lista de skills é lida do disco pelo núcleo do app, e o plugin recebe só o nome e a descrição de cada uma.
- O acesso à lista é uma permissão nova, declarada pelo plugin no manifesto. Sem ela, o plugin não consegue ler a lista.
- A escolha das skills é feita sem chamar o modelo, para não gastar tokens na própria escolha.
- O plugin não remove skills da lista do CLI: a dica é a única forma de influenciar o agente.
- O histórico da conversa não muda nesta etapa; a decisão depende de medição com o painel de cache.
- O plugin vale para Claude e Codex.
- Nenhuma dependência nova é adicionada.

## Testing Decisions

- Com uma skill cuja descrição bate com o pedido, a dica a inclui; com um pedido sem relação, a dica não aparece.
- O pedido enviado ao agente é o mesmo que o usuário escreveu, com a dica acrescentada antes ou depois dele.
- Um plugin sem a permissão de skills não consegue ler a lista.
- Uma pasta de skill sem descrição válida não aparece na lista.
- Um plugin desligado não acrescenta dica nenhuma.

## Out of Scope

- Desligar skills por turno no CLI, porque o CLI só oferece desligar todas de uma vez.
- Mudar o histórico da conversa ou o uso de --resume.
- Mudar a compactação automática.
- Mudar o formato das conversas salvas.
- Corrigir o teste de estilo do composer que já falha no main.
- Reescrever linhas antigas do MEMORY.md.

## Further Notes

- O plano aprovado está em `luna-plano` na conversa. A spec de ordenação de projetos continua em `spec.md`; esta fica em arquivo separado para não sobrescrevê-la.
- O CLI tem `--disable-slash-commands`, que desliga todas as skills numa invocação. Ele não serve para escolher skills, por isso não é usado aqui.
