# Régua de tela

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** M

## O que é

Mede distâncias em pixels, com setas nas pontas e o valor no meio.

## Como fazer

Nova `Shape::Ruler { a, b }`. A geometria é uma linha; o rótulo reaproveita
o badge que o overlay já usa para as dimensões da seleção. Medir em pixels
da imagem, não em pontos: a 150% de escala os números diferem.
