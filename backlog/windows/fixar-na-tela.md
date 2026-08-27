# Fixar a captura na tela

**Plataforma:** windows · **Estado:** falta · **Esforço:** M

## O que é

A captura vira uma janelinha sem bordas, sempre no topo, até ser fechada.

## Como fazer

Janela `WS_EX_TOPMOST | WS_EX_TOOLWINDOW` sem bordas, arrastável pelo corpo.
O processo de GUI já sobe viewports próprios: é um modo novo de `--gui`.

## Notas

O item mais pedido de quem usa este tipo de app. Ficou de fora do port do omasnap por decisão explícita.
