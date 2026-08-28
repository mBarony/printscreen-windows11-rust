# Seleção inteligente

**Plataforma:** windows · **Estado:** feito · **Esforço:** G

## O que é

Detecta o elemento sob o cursor e ajusta a seleção sozinha.

## Como fazer

Duas fontes, com qualidades diferentes:

- `EnumChildWindows` dá os controles nativos, mas não enxerga nada desenhado
  por conta própria (Electron, Qt, jogos).
- Detecção por imagem: achar retângulos de contraste homogêneo. Funciona em
  qualquer coisa, mas erra mais.

O Shottr usa o segundo caminho.

## Como ficou

Entregue em 28/08/2026: no overlay de seleção, `Espaço` passou a **ciclar três modos** — arrastar à mão, escolher uma janela inteira e **escolher o elemento sob o cursor**. O elemento fica destacado sem véu, como a janela já ficava, e o clique confirma.

**Detecção por imagem, e não `EnumChildWindows`.** O caminho nativo devolve os controles com precisão e não enxerga nada desenhado por conta própria: acertaria tudo no Bloco de Notas e nada no VS Code, no navegador ou em qualquer coisa em Electron ou Qt — que é onde as capturas de tela realmente acontecem.

**A ideia é que um elemento de interface é uma superfície de cor uniforme**: o fundo de um botão, de um painel, de uma barra. A região conectada dessa cor a partir do cursor, e a caixa dela, dão o elemento. O texto e os ícones de dentro viram buracos na região e não atrapalham — a caixa os contém.

**A cor da superfície é a dominante em volta do cursor, não a de baixo dele.** Sobre uma letra, a de baixo é a tinta do glifo, e a região conectada seria a própria letra: o teste `o_cursor_sobre_o_texto_devolve_o_botao_e_nao_a_letra` existe por isso.

**Uma superfície que atravessa o alcance da busca nos dois eixos é recusada.** Isso é o fundo da tela, e selecionar o fundo não ajuda ninguém — além de manter o custo por quadro previsível, já que a busca nunca varre mais que 841×841 px.

**A caixa só é recalculada quando o ponteiro anda 3 px.** A cada quadro seria uma inundação de meio megapixel para devolver a mesma resposta.

A tolerância de 12 por canal aceita os degradês sutis e o anti-aliasing que toda interface tem; zero recusaria o próprio botão.

O módulo é lógica pura sobre uma imagem — testa-se sem Windows, sem GPU e sem desktop, e é o que os sete testes fazem com uma "tela" sintética de janela, botão e texto.
