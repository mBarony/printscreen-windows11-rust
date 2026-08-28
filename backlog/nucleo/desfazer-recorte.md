# Desfazer o recorte

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

Voltar ao enquadramento original sem desfazer o resto do trabalho.

## Como fazer

O replay já parte da imagem pristina e os `Crop` são operações — a
informação está toda lá. Basta remover os `Crop` do log e refazer o replay.
Atenção ao teto do histórico: o primeiro `Crop` é imune ao descarte.

## Como ficou

Entregue em 28/08/2026: um botão na barra remove os `Op::Crop` do log e
refaz o replay, que já partia da imagem pristina — a informação estava toda
lá, como o backlog previa.

Duas decisões que o esboço não cobria:

- O "refazer" pendente é descartado junto. Reconstruir o log sem os recortes
  deixaria as operações à frente apontando para um enquadramento que não
  existe mais.
- Um recorte já **consolidado** na imagem de partida pelo teto do histórico
  (100 operações) não volta, e a função devolve `false` nesse caso. Ele
  deixou de ser reversível quando foi assado na base.
