# Melhorias: abertura sem pedido desnecessário, aprovação de plano, velocidade, abas por projeto e tarefas no navegador

## Problem Statement

Cinco problemas de uso no LunaHarness:

1. Ao abrir o app, a tela de instalação das CLIs aparece sempre que pelo menos uma CLI suportada falta. Quem já tem uma CLI funcionando vê o pedido toda vez.
2. No modo de planejamento, o card "Aprovar e executar" aparece depois de qualquer resposta, mesmo quando o plano ainda não está pronto.
3. A velocidade de resposta fica atrás de um botão e vem desligada. O usuário quer vê-la sempre, do mesmo jeito em todas as CLIs.
4. O navegador embutido é único para o app inteiro. Com dois projetos abertos, o localhost de um aparece no outro.
5. Não há garantia de que o agente conclua tarefas completas no navegador, como melhorar um texto e publicar no LinkedIn, nem de que consiga enviar um arquivo para um formulário.

## Solution

- Abertura: a tela de instalação das CLIs só aparece quando nenhuma CLI suportada está instalada. Com pelo menos uma instalada, o app abre direto. As CLIs que faltam continuam instaláveis pelas Configurações.
- Plano: o card de aprovação só aparece quando a resposta do modo de planejamento traz o plano pronto.
- Velocidade: a velocidade fica visível sempre que houver medição, sem botão. Vale para todas as CLIs. Na Codex, que informa a saída só ao fim do turno, aparece sem dados durante a resposta e com o valor final ao terminar.
- Abas: cada projeto tem o próprio conjunto de abas do navegador, com limite de 8 abas por projeto. Ao trocar de projeto, as abas do outro projeto ficam guardadas e voltam quando o usuário volta. Login e cookies continuam compartilhados.
- Tarefas no navegador: o agente deve conseguir preencher um formulário e acionar o botão de publicar numa página de teste local, com a permissão da pessoa antes de cada ação sensível. O envio de arquivo entra só se um teste de viabilidade confirmar que funciona; se não funcionar, fica para depois e o usuário é avisado.

## User Stories

1. Como usuário que já tem uma CLI instalada, quero abrir o app sem o pedido de instalação, para começar a conversar direto.
2. Como usuário sem nenhuma CLI instalada, quero ver a tela de instalação ao abrir o app, para escolher o que instalar, e poder pulá-la.
3. Como usuário com uma CLI instalada, quero continuar podendo instalar as outras pelas Configurações.
4. Como usuário no modo de planejamento, quero ver o botão de aprovação só quando o plano estiver pronto, para não aprovar um rascunho.
5. Como usuário, quero ver a velocidade da resposta sem precisar ligar nada, para acompanhar o desempenho de qualquer CLI.
6. Como usuário com Codex, quero saber que a velocidade só aparece ao fim da resposta, sem um número falso no meio do caminho.
7. Como usuário com dois projetos, quero que o navegador mostre só as abas do projeto em que estou, para não confundir um localhost com outro.
8. Como usuário, quero que as abas de um projeto voltem exatamente como deixei quando volto a ele.
9. Como usuário, quero pedir ao agente que publique um texto numa página e ser pedido de permissão antes do clique em publicar.
10. Como usuário, quero, se o envio de arquivo não funcionar ainda, ser avisado de que não há suporte, em vez de ver o agente falhar em silêncio.

## Implementation Decisions

- O frontend é feito em React com TypeScript, como no restante do app. Nenhum outro framework de interface é introduzido.
- O núcleo do app é feito em Rust. A detecção e a instalação das CLIs não mudam.
- A regra de abertura da tela de instalação é uma função pura, sem efeito colateral, que recebe a situação das CLIs e responde se a tela deve abrir. Uma lista vazia não abre a tela.
- A preferência de mostrar a velocidade deixa de existir: a velocidade é sempre exibida quando há medição. A chave antiga guardada no computador do usuário não é apagada, apenas deixa de ser lida.
- A velocidade é tokens de saída do turno divididos pelo tempo desde o primeiro texto da resposta. A origem dos tokens é a informação que cada CLI devolve; o Luna não estima.
- Cada projeto tem o próprio conjunto de abas do navegador. O limite de abas vale por projeto. O núcleo continua dono das abas; o front só mostra e pede.
- O perfil do navegador (login, cookies, sessões) é único para o app. Só as abas são separadas.
- As ações do agente no navegador passam pela mesma verificação de ação sensível que já existe: ação sensível só acontece depois da permissão da pessoa no chat da conversa.
- Envio de arquivo, se implementado, usa o mecanismo de automação do próprio navegador embutido. Enquanto não houver teste que confirme esse mecanismo, o envio não é implementado.
- Nenhuma API pública nova é criada para plugins. Os comandos existentes do navegador passam a receber o projeto onde atuam.
- Cada item de melhoria sai em um branch próprio, com commits separados no padrão Conventional Commits.

## Testing Decisions

- Com nenhuma CLI instalada, a tela de instalação aparece ao abrir o app, e permite pular sem instalar nada.
- Com pelo menos uma CLI instalada, a tela não aparece ao abrir o app, mesmo que outras faltem.
- Uma falha na instalação mostra o erro na tela, sem apagar o que já estava instalado.
- No modo de planejamento, uma resposta sem plano não mostra o botão de aprovação; uma resposta com plano mostra.
- A velocidade aparece sem nenhum botão, para Claude e para Codex. Na Codex, não aparece durante a resposta e aparece ao fim.
- Abas de um projeto não aparecem em outro projeto. Trocar de projeto e voltar mantém as abas do primeiro.
- Um projeto não passa de 8 abas; a nona recusa com mensagem clara.
- Uma ação de clique em botão de publicar não acontece sem a permissão da pessoa.
- Uma página local de teste com formulário e botão de publicar é preenchida e publicada pelo agente depois da permissão.
- Se o envio de arquivo não tiver teste de viabilidade aprovado, o agente recebe uma resposta clara de que a ação não é suportada.

## Out of Scope

- Mudar a detecção ou a instalação das CLIs no núcleo.
- Lembrar, entre aberturas, que o usuário dispensou a tela de instalação.
- Persistir abas entre aberturas do app.
- Compartilhar login, cookies ou sessões entre projetos.
- Contar ou estimar tokens economizados.
- Escolha em lista suspensa no navegador do agente.
- Publicação real em redes sociais durante os testes automáticos; o teste usa página local.
- Corrigir o teste de estilo do composer que já falha no main, sem relação com estas mudanças.

## Further Notes

- Usuários com apenas uma CLI instalada deixam de ver o aviso de instalação na abertura. A instalação das outras fica disponível pelas Configurações.
- A velocidade da Claude vem do valor de saída de cada chamada ao modelo. A documentação da Anthropic diz que esse valor é acumulado dentro da mesma mensagem; a implementação precisa garantir que não é somado duas vezes.
- O envio de arquivo não tem caminho confirmado na documentação oficial do WebView2. A decisão depende do resultado do teste de viabilidade.
- Limite de 8 abas por projeto pode somar vários processos do WebView2 quando há muitos projetos abertos. Isso precisa ser avisado ao usuário.
