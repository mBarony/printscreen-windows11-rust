# Seta em arco

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** M

## O que é

Uma alça no meio da seta a dobra num arco. No Shottr toda seta pode ser dobrada.

## Como fazer

`Shape::Arrow` ganha um ponto de controle opcional; com ele, o traço vira
Bézier quadrática. O rasterizador já amostra Béziers para o traço à mão
livre. A ponta é desenhada na tangente do fim, e o `hit_test` passa a medir
distância à curva amostrada.

## Como ficou

Entregue em 28/08/2026. A seta ganhou uma terceira alça, no meio: arrastá-la
dobra o traço num arco.

**A dobra é fração do comprimento, não deslocamento absoluto.** Assim setas
de tamanhos diferentes ficam com a mesma curvatura aparente — com um valor
absoluto, uma seta curta viraria laço e uma longa pareceria reta. É limitada
a ±0,6: além disso o arco fecha e a ponta deixa de apontar para o alvo, que
é o motivo de existir uma seta.

**A ponta segue a tangente do fim da curva**, não a direção da corda. Com a
seta dobrada, apontá-la pela corda deixaria a farpa torta em relação ao
traço que chega nela. A haste é encurtada até a base da ponta para não vazar
por dentro dela.

A curva é uma Bézier quadrática amostrada em 24 segmentos, e o controle fica
no **dobro** da distância do vértice — uma quadrática só chega à metade do
caminho até o ponto de controle.

O `hit_test` mede distância aos segmentos amostrados, então clicar no arco
funciona onde ele de fato está. Sessões gravadas antes disto abrem com
`bend: 0`, ou seja, retas como eram.
