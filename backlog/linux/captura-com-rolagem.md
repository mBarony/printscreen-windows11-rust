# Captura com rolagem

**Plataforma:** linux · **Estado:** falta · **Esforço:** G

## O que é

Equivalente Linux da captura de página longa.

## Como fazer

Não há como enviar eventos de rolagem a uma janela alheia em Wayland — é
proibido por design. A saída é `hyprctl dispatch` ou `wtype`/`ydotool`, que
injetam no compositor.

A costura dos quadros é a mesma do Windows e vem de `windows/captura-com-rolagem`.

## Notas

Mais frágil que no Windows: depende de ferramenta externa de injeção.
