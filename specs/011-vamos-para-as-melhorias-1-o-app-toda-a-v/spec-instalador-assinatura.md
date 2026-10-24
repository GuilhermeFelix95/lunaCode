# Instalador da Luna com marca própria e assinatura gratuita

## Problem Statement

O instalador do Windows mostra as imagens padrão do NSIS, por isso parece genérico e não é reconhecível como o app da Luna. Além disso, ele aparece como de editor desconhecido, porque não é assinado. O projeto é de código aberto e quer um instalador com a marca da Luna e assinatura sem custo.

## Solution

- O instalador usa o ícone, a imagem do cabeçalho e a imagem lateral com a marca da Luna.
- O executável do Windows é assinado por um programa gratuito para projetos de código aberto.
- O editor exibido no Windows passa a ser o do programa de assinatura, não o da Luna.
- A assinatura das atualizações do app não muda.
- O macOS fica de fora por enquanto.

## User Stories

1. Como usuário que baixa a Luna, quero que o instalador tenha a cara do app, para reconhecer que é o instalador oficial.
2. Como usuário do Windows, quero que o instalador mostre um editor confiável, para não desconfiar do aviso de editor desconhecido.
3. Como usuário com a Luna já instalada, quero que as atualizações continuem funcionando depois da mudança.
4. Como mantenedor, quero que a política de assinatura esteja no README, para que quem avaliar o projeto saiba como o instalador é assinado.

## Implementation Decisions

- O instalador é gerado pelo mesmo fluxo de release atual, sem nova ferramenta de empacotamento.
- As imagens do instalador seguem a marca da Luna e os tamanhos exigidos pelo instalador NSIS.
- A assinatura do Windows usa o programa da SignPath Foundation, que exige licença OSI, projeto mantido e política de assinatura pública.
- A chave de assinatura das atualizações do app é mantida; trocá-la quebraria as atualizações já instaladas.
- A assinatura é feita pelo processo de release, e não manualmente a cada publicação.
- Nenhuma dependência nova é adicionada ao código do app.

## Testing Decisions

- O instalador gerado mostra as imagens da Luna em todas as telas.
- O arquivo do instalador traz assinatura válida do programa de assinatura, verificável pelo próprio Windows.
- Depois da assinatura, o editor exibido ao instalar é o do programa de assinatura, e não "editor desconhecido".
- As atualizações de uma versão já instalada continuam sendo aceitas.
- Uma instalação em máquina limpa conclui sem erro.

## Out of Scope

- Assinatura para macOS, que exige conta paga.
- Troca da chave de assinatura das atualizações.
- Mudança do ícone do executável, que já é o da Luna.
- Republicar releases antigas.
- Mudanças no chat, nos plugins ou nos provedores.
- Corrigir o teste de estilo do composer que já falha no main.
- Reescrever linhas antigas do MEMORY.md.

## Further Notes

- O plano aprovado está em `luna-plano` na conversa; este arquivo registra o que o trabalho deve entregar.
- A inscrição na SignPath Foundation é feita pelo mantenedor, com conta e autenticação em dois fatores próprias. Nenhum dado pessoal é enviado pelo projeto.
- A spec de ordenação de projetos continua em `spec.md`, sem alteração.
