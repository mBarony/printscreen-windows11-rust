# Cor do texto sob o cursor

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

O tom mais escuro num quadrado de 20×20 — num texto, a cor da letra.

## Como fazer

Percorrer a vizinhança e devolver o pixel de menor luminância.

## Como ficou

Entregue em 27/08/2026: `Shift`+clique com o conta-gotas devolve o tom mais
escuro num quadrado de 20×20 px em volta do cursor, em
`crate::color::darkest_around`.

É o complemento útil do clique simples, que sobre texto quase sempre pega o
fundo — a letra ocupa menos área que o espaço em volta dela.
