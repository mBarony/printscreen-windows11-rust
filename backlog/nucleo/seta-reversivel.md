# Seta reversível

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

Inverte de que lado fica a ponta, sem redesenhar.

## Como fazer

Trocar `a` e `b` na `Shape::Arrow` selecionada, como operação `Patch` do op-log.

## Como ficou

Entregue em 28/08/2026: `Alt+R` com uma seta selecionada troca de que lado
fica a ponta.

Trocar as extremidades é toda a operação — a ponta é sempre desenhada em `b`
e o resto da geometria é simétrico. Entra pela porta do movimento
(`begin_move`/`end_move`), então vira um passo de desfazer; se a anotação
selecionada não for uma seta, o movimento é abortado e o histórico não é
tocado.
