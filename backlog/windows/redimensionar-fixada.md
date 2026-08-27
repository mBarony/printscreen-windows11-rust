# Redimensionar a janela fixada

**Plataforma:** windows · **Estado:** falta · **Esforço:** P
**Depende de:** fixar-na-tela

## O que é

A roda sobre a janela fixada aumenta e diminui o tamanho.

## Como fazer

Tratar `WM_MOUSEWHEEL` na janela, escalando o tamanho externo e reamostrando.
