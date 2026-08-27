# Estilos de linha

**Plataforma:** nucleo · **Estado:** parcial · **Esforço:** M

## O que é

Tracejado e pontilhado além do sólido, que hoje é o único.

## Como fazer

Em `editor/raster/stroke.rs`, um padrão aplicado ao comprimento acumulado da
polilinha — mesmo ponto onde a espessura já entra. O estilo vira campo do
`Style` e entra no `Patch`.
