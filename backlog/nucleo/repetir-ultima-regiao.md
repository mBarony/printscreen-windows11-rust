# Repetir a última região

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

Recaptura o mesmo retângulo da vez anterior, sem arrastar de novo.

## Como fazer

Guardar o último `rect` e acrescentar um modo que pula o overlay.

## Como ficou

Entregue em 28/08/2026: item "Repetir a última região" no menu da bandeja,
que só aparece depois da primeira região capturada.

O retângulo é guardado em **coordenadas absolutas do desktop virtual**, num
arquivo ao lado do `config.json` (`src/last_region.rs`). Quem grava é o
processo de GUI e quem lê é o residente, e entre um e outro a lista de
monitores pode ter mudado — um índice de monitor apontaria para outro lugar.

Ao repetir, o monitor que contém o canto superior esquerdo manda. Um
retângulo que atravessa dois monitores já não era representável na seleção,
então aqui também não precisa ser; se nenhum monitor contiver o ponto, um
aviso explica que a tela mudou em vez de capturar a região errada.
