# Ordenação alfabética dos projetos na barra lateral

## Problem Statement

A barra lateral ordena os projetos pelo uso mais recente. Com isso, a lista muda de posição sempre que uma conversa é usada, e fica difícil achar um projeto pelo nome. O usuário quer uma ordem previsível: alfabética, pelo nome da pasta do projeto.

## Solution

- Os projetos da barra lateral ficam em ordem alfabética pelo nome da pasta.
- A comparação ignora maiúsculas e acentos. "Álbum" vem antes de "Alfa", porque a comparação segue as letras base (a antes de f).
- Se dois projetos têm o mesmo nome de pasta, o desempate usa o caminho completo.
- Dentro de cada projeto, as conversas continuam da mais recente para a mais antiga.
- A ordem alfabética vale só para a lista de projetos. Nenhum projeto fica fixo no topo.

## User Stories

1. Como usuário, quero ver meus projetos em ordem alfabética, para achar um projeto pelo nome sem procurar na lista inteira.
2. Como usuário, quero que a ordem não mude quando eu uso uma conversa, para que a lista fique estável.
3. Como usuário com nomes com maiúsculas ou acentos, quero que a ordem trate esses nomes como as letras base, para não ver "Álbum" depois de "Zebra" por causa do acento.
4. Como usuário com duas pastas de mesmo nome em lugares diferentes, quero que a lista as separe de forma previsível.
5. Como usuário, quero que as conversas dentro de cada projeto continuem na ordem da mais recente para a mais antiga.

## Implementation Decisions

- O frontend é feito em React com TypeScript, como no restante do app. Nenhum outro framework de interface é introduzido.
- A ordenação é uma regra de apresentação da barra lateral. Ela não muda os dados salvos das conversas nem o núcleo do app.
- A comparação de nomes usa a ordenação de texto do próprio sistema, sem bibliotecas novas.
- O nome exibido de cada projeto continua sendo o último segmento da pasta.
- O agrupamento das conversas por dia dentro do projeto não muda.
- Nenhuma dependência nova é adicionada.

## Testing Decisions

- Com projetos "Alfa" e "Álbum", a lista mostra "Álbum" antes de "Alfa".
- Nomes que diferem só em maiúsculas (ex.: "beta" e "Beta") são tratados como equivalentes na ordem; o desempate usa o caminho completo.
- Com duas pastas de mesmo nome em caminhos diferentes, a ordem entre elas é a do caminho completo.
- Dentro de um projeto, a conversa mais recente aparece primeiro.
- Usar uma conversa de um projeto não muda a posição dos projetos na lista.

## Out of Scope

- Ordenar conversas dentro do projeto por nome.
- Fixar um projeto no topo, como o que tem a conversa aberta.
- Ordenação por idioma diferente do padrão do sistema.
- Mudar o agrupamento por dia dentro do projeto.
- Mudar o núcleo do app, a API de plugins ou o formato das conversas salvas.
- Corrigir o teste de estilo do composer que já falha no main.
- Reescrever linhas antigas do MEMORY.md.

## Further Notes

- A spec de perguntas (bloco malformado escondido com aviso e modo sem o campo de perguntas mostrando o seletor) já está atendida neste branch pelos commits 28b4400 e fc450be. Este trabalho só a revisa, testa e registra.
- A ordem "Álbum" antes de "Alfa" foi confirmada pelo usuário, que pediu ordem pelas letras base, sem acento e sem maiúscula.
- O formato das novas linhas do MEMORY.md passa a ser `AAAA-MM-DD: o que foi feito`, conforme o CLAUDE.md. As linhas antigas ficam como estão.
