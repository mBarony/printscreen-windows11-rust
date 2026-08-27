# Desfazer o recorte

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** P

## O que é

Voltar ao enquadramento original sem desfazer o resto do trabalho.

## Como fazer

O replay já parte da imagem pristina e os `Crop` são operações — a
informação está toda lá. Basta remover os `Crop` do log e refazer o replay.
Atenção ao teto do histórico: o primeiro `Crop` é imune ao descarte.
