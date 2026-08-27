# Lista de janelas

**Plataforma:** linux · **Estado:** falta · **Esforço:** P

## O que é

Enumerar janelas visíveis para capturar uma inteira.

## Como fazer

`hyprctl clients -j` devolve JSON com geometria, título e classe de cada
janela. O `json.rs` já lê JSON, então é um `Command` e um parse.

## Notas

Em GNOME/KDE isto seria impossível; em wlroots é um comando de linha.
