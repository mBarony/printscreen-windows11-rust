# Backlog

Uma feature por arquivo, separada por plataforma. Levantado da
[página do Shottr](https://shottr.cc) e confrontado com o que o RustShot
v1.8.0 já faz — o que já existe não está aqui.

**Esforço:** P = até um dia · M = alguns dias · G = uma semana ou mais

| Pasta | O que é | Itens |
|---|---|---|
| [`nucleo/`](nucleo/README.md) | Núcleo | 26 |
| [`windows/`](windows/README.md) | Windows | 7 |
| [`linux/`](linux/README.md) | Linux (Hyprland + Wayland) | 12 |
| [`macos/`](macos/README.md) | macOS | 13 |

Total: **58 itens**.

O andamento é registrado em [controle_backlog.md](controle_backlog.md).

## Como ler

A maior parte das funcionalidades é de **núcleo**: o editor, o op-log, o
rasterizador e os codificadores não dependem de sistema operacional, então
implementar uma vez vale para as três plataformas. As pastas por sistema
guardam duas coisas: as capacidades de plataforma (captura, área de
transferência, atalhos…) e as poucas features cuja implementação difere de
verdade entre sistemas.

## Ordem sugerida

1. `windows/fixar-na-tela` — o mais pedido, sem armadilha conhecida.
2. `nucleo/formato-png` — corrige o pior defeito da saída atual.
3. `nucleo/leitura-de-qr-code` — barato e distintivo.
4. Os itens P de `nucleo/` — vários pequenos que somam.
5. `nucleo/seta-em-arco` e `nucleo/estilo-desenhado-a-mao`.
6. `windows/captura-com-rolagem` — o maior, e o que mais diferencia.
7. `camada-de-plataforma` (abaixo), depois `linux/` e `macos/`.

## Pré-requisito do porte

Antes de escrever a segunda plataforma, extrair uma interface por
capacidade em `src/platform/`, com o código Win32 de hoje virando a
implementação `cfg(windows)`. São ~15 módulos, trabalho mecânico — e é o
que decide se o porte seguinte custa semanas ou meses. Uma interface
desenhada olhando só para o Windows nasce torta.
