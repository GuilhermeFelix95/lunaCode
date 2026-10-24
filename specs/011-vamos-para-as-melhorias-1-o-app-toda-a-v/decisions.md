# Decisões

## Rodada 1
- Item 1: quando o card "Aprovar e executar" deve aparecer no modo plano?: Só quando a resposta traz o plano pronto
- Item 2: onde a velocidade deve aparecer para as CLIs que ainda não validamos (Codex e as outras)?: Aparece em todas, do mesmo jeito que na Claude
- Item 2: a velocidade deve continuar visível depois que a resposta termina, até o próximo turno?: Sim, como hoje
- Item 3: o que deve ser separado por projeto no navegador?: Só as abas; o login e os cookies continuam compartilhados
- Item 4: qual foi o caso que falhou ao pedir para postar no X?: nao testei é so por garantia que o agent faça isso, exemplo pedir pra melhorar o linkedin, ele faz isso, publicar um app coisa desse tipo
- Item 4: ao postar pelo navegador, o app deve continuar pedindo sua permissão antes de clicar em Postar?: ele pergunta no seletor de perguntas com opção de postar, vou revisar e sempre aprovar interação no site

## Rodada 2
- Item 2: com o Codex, a velocidade só existe no fim do turno. O que mostrar durante a resposta dele?: Sem dados até terminar, e o valor final ao fim do turno
- Item 3: o limite de 8 abas do navegador é por projeto ou no total?: 8 por projeto
- Item 3: o que acontece com as abas de um projeto quando eu troco para outro?: Ficam guardadas e voltam quando eu volto ao projeto
- Item 4: como testar se o agente faz o post completo (ex.: melhorar e publicar no LinkedIn)?: Página local de teste com formulário e botão de publicar
- Item 4: incluir no escopo o envio de arquivo (anexar imagem) e a escolha em lista suspensa no navegador do agente?: Só envio de arquivo

## Rodada 3
- Item 4: o envio de arquivo ainda não tem caminho confirmado no WebView2. Como tratar?: Fazer um teste de viabilidade antes; se não funcionar, deixo para depois e aviso
- Os quatro itens devem ir em branches e commits separados, um por item?: Sim, um branch e commits por item
