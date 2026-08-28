# Outros formatos de cor

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

OKLCH e contraste APCA além do HEX.

## Como fazer

sRGB → OKLab → OKLCH é aritmética fechada. O formato preferido entra no `config.json`.

## Como ficou

Entregue em 27/08/2026. A dica do botão de cor passa a mostrar, além do HEX:

- **OKLCH**, perceptualmente uniforme — dois tons com a mesma claridade
  parecem igualmente claros, o que o HSL não garante.
- **Contraste APCA** da cor sobre branco e sobre preto, que responde
  diretamente "dá para ler texto nesta cor?". Em geral pede-se |Lc| ≥ 60 para
  texto de corpo.

O APCA é assimétrico de propósito, ao contrário do WCAG 2: texto escuro sobre
claro e claro sobre escuro não se leem igual, e o número reflete isso. Há
teste garantindo essa assimetria.
