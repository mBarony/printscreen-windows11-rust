# Duplicar arrastando

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

`Alt`+arrasto duplica e move a cópia, em vez do `Alt+D` com deslocamento fixo.

## Como fazer

Ao iniciar arrasto de corpo com o modificador, criar a cópia (id, semente e número novos) e arrastar a cópia.

## Como ficou

Entregue em 28/08/2026: `Alt`+arrasto sobre uma anotação duplica em vez de
mover.

A cópia nasce **sem deslocamento**, por cima do original, e é ela que segue o
ponteiro — o original fica onde estava. É o `Alt+D` sem ter de reposicionar
depois. A cópia recebe id novo, semente nova na redação e número novo no
marcador, como a duplicação já fazia.
