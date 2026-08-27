# Fixar a captura na tela

**Plataforma:** windows · **Estado:** feito · **Esforço:** M

## O que é

A captura vira uma janelinha sem bordas, sempre no topo, até ser fechada.

## Como fazer

Janela `WS_EX_TOPMOST | WS_EX_TOOLWINDOW` sem bordas, arrastável pelo corpo.
O processo de GUI já sobe viewports próprios: é um modo novo de `--gui`.

## Notas

O item mais pedido de quem usa este tipo de app. Ficou de fora do port do omasnap por decisão explícita.

## Como ficou

Entregue em 27/08/2026.

- Botão na barra do editor, junto de copiar e salvar, e atalho `Ctrl+P`.
- `src/pinned.rs`: janela sem decoração, `always_on_top`, arrastável pelo
  corpo inteiro (`ViewportCommand::StartDrag`) — sem barra de título, não há
  outro lugar por onde pegar.
- Nasce encolhida quando a captura é maior que 520 pt no lado maior: uma tela
  cheia fixada em tamanho natural cobriria o monitor e seria inútil.
- A roda redimensiona entre 15% e 400%, de forma multiplicativa — um passo
  aditivo faria a janela saltar quando pequena.
- Fecha com `Esc`. Enquanto estiver aberta, segura o processo de GUI vivo,
  como o aviso do OCR já fazia.

Fica para depois: [redimensionar-fixada](redimensionar-fixada.md) já saiu
junto (a roda), e várias janelas fixadas ao mesmo tempo — hoje é uma só.
