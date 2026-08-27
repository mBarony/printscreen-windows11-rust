# Seleção inteligente

**Plataforma:** windows · **Estado:** falta · **Esforço:** G

## O que é

Detecta o elemento sob o cursor e ajusta a seleção sozinha.

## Como fazer

Duas fontes, com qualidades diferentes:

- `EnumChildWindows` dá os controles nativos, mas não enxerga nada desenhado
  por conta própria (Electron, Qt, jogos).
- Detecção por imagem: achar retângulos de contraste homogêneo. Funciona em
  qualquer coisa, mas erra mais.

O Shottr usa o segundo caminho.
