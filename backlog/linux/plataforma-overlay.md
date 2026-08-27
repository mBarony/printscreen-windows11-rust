# Overlay de seleção

**Plataforma:** linux · **Estado:** falta · **Esforço:** M

## O que é

A janela sem bordas, sempre no topo, onde se arrasta a região.

## Como fazer

`wlr-layer-shell` na camada Overlay — o equivalente exato do "sempre no
topo" do Windows. É o que o omasnap usa, via `LayerShellQt`.

Em Rust há binding para o protocolo; o eframe/winit precisa aceitar a
superfície, o que é o ponto a verificar cedo.
