# Cor média de uma área

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

Em vez de um pixel, a média de uma região arrastada.

## Como fazer

Extensão do conta-gotas: somar os canais da região e dividir.

## Como ficou

Entregue em 27/08/2026. Arrastar com o conta-gotas amostra a média do
retângulo, em `crate::color::average`.

A média é aritmética por canal no espaço sRGB, não no linear: é o que bate
com a expectativa de quem olha a tela — a cor "no meio" das que aparecem, e
não a média física de luz, que puxa para o claro.

O retângulo é aceito em qualquer sentido de arrasto e é recortado à imagem.
