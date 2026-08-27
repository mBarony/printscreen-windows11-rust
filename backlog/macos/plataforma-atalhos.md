# Atalhos globais

**Plataforma:** macos · **Estado:** falta · **Esforço:** M

## O que é

Disparar as capturas por tecla, sem foco.

## Como fazer

`RegisterEventHotKey` (Carbon, ainda a via suportada) ou
`NSEvent.addGlobalMonitorForEvents`, que exige permissão de
**Acessibilidade**.
