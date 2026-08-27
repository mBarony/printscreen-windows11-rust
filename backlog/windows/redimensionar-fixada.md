# Redimensionar a janela fixada

**Plataforma:** windows · **Estado:** feito · **Esforço:** P
**Depende de:** fixar-na-tela

## O que é

A roda sobre a janela fixada aumenta e diminui o tamanho.

## Como fazer

Tratar `WM_MOUSEWHEEL` na janela, escalando o tamanho externo e reamostrando.

## Como ficou

Entregue junto de [fixar-na-tela](fixar-na-tela.md) em 27/08/2026: a roda
sobre a janela ajusta a escala entre 15% e 400%, e o viewport é
redimensionado por `ViewportCommand::InnerSize`.

O passo é multiplicativo, não aditivo — perto do mínimo um passo fixo faria a
janela saltar de tamanho.
