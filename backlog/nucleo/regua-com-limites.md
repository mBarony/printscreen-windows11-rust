# Régua que acha os limites sozinha

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** M
**Depende de:** regua-de-tela

## O que é

Com a régua ativa, mover o ponteiro sobre um elemento faz a medida se estender
sozinha até os limites dele — horizontais ou verticais conforme a direção do
movimento. Em vez de arrastar de ponta a ponta à mão, o usuário aponta e a
régua já mostra a largura daquele botão, daquela coluna, daquela margem.

## Como fazer

O eixo sai do **movimento recente do ponteiro**: predominou o deslocamento em
x, mede-se a largura; predominou em y, a altura. A borda sai da **diferença de
pixel**: varrer a partir do cursor nos dois sentidos do eixo até a cor deixar
de parecer com a de partida.

A parte difícil já existe em `smartpick.rs`, que faz exatamente esse tipo de
busca para a seleção por elemento: `cor_dominante` (a cor da superfície em
volta do cursor, e não a de baixo dele — sobre uma letra, a de baixo é a tinta
do glifo), `vizinho_da_superficie` e `parecido`. Reaproveitar, e não escrever
um segundo detector com outro critério de "mesma cor" — dois detectores que
discordam é pior que um imperfeito.

As duas posições achadas viram `a` e `b` da `Shape::Ruler` que a ferramenta já
tem. Clicar congela como anotação normal, que se move, se ajusta pelas alças e
desfaz como qualquer outra.

## Decisões em aberto

**Movimento diagonal.** Sem eixo dominante claro não há o que medir. As saídas
são recusar (a régua fica como está até o movimento se definir) ou fixar no
último eixo válido. A segunda é menos surpreendente, mas mascara o caso em que
o usuário quer trocar de eixo.

**Modo ou padrão.** A medida automática pode ser o comportamento da régua
enquanto ninguém arrasta, ou um modo separado por tecla. O primeiro é mais
direto e é o que o pedido descreve; o risco é a régua ficar "viva" na tela e
disputar a atenção enquanto o usuário só passa o mouse para chegar noutro
lugar.

**Superfície que atravessa a tela.** O `smartpick` recusa quando a superfície
ultrapassa o alcance da busca nos dois eixos — isso é o fundo da tela, não um
elemento. A régua precisa da mesma recusa, senão medirá a janela inteira
sempre que o cursor cair num espaço vazio.
