# Guias de alinhamento

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

Linhas de apoio horizontais e verticais para alinhar anotações.

## Como fazer

Lista de coordenadas no estado do editor, desenhadas por cima e ignoradas na exportação.

## Como ficou

Entregue em 28/08/2026: `Alt+H` e `Alt+V` criam uma guia na posição do
cursor; `Alt+Shift+G` limpa todas.

São ajuda visual e nada mais: não entram no op-log, não viram passo de
desfazer e não aparecem na exportação.

Desenhadas depois da imagem e **antes** das anotações — são apoio para
posicionar, então não podem tapar o que se está posicionando.

O canvas é quem sabe onde o cursor está, e o atalho é tratado longe dele;
por isso a posição fica num campo (`guide_hint`) atualizado a cada quadro.
