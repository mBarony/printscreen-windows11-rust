# Seta em arco

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** M

## O que é

Uma alça no meio da seta a dobra num arco. No Shottr toda seta pode ser dobrada.

## Como fazer

`Shape::Arrow` ganha um ponto de controle opcional; com ele, o traço vira
Bézier quadrática. O rasterizador já amostra Béziers para o traço à mão
livre. A ponta é desenhada na tangente do fim, e o `hit_test` passa a medir
distância à curva amostrada.
