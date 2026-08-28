# Estilos de linha

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** M

## O que é

Tracejado e pontilhado além do sólido, que hoje é o único.

## Como fazer

Em `editor/raster/stroke.rs`, um padrão aplicado ao comprimento acumulado da
polilinha — mesmo ponto onde a espessura já entra. O estilo vira campo do
`Style` e entra no `Patch`.

## Como ficou

Entregue em 28/08/2026. `Style` ganhou o campo `line`, e um botão da barra cicla sólido → tracejado → pontilhado. Vale para linha, seta, retângulo, elipse, mão livre, marca-texto e régua; com algo selecionado, repinta a anotação como os outros controles de estilo.

**O padrão é medido em múltiplos da espessura, não em pixels fixos.** Um tracejado de 6 px sobre um traço de 12 px de largura sai como uma fila de quadrados colados, sem leitura de "tracejado" nenhuma.

**O período é esticado para caber um número inteiro de vezes no caminho**, e o caminho começa e termina com tinta. Sem isso, a última esquina de um retângulo tracejado cairia no meio de uma folga — justamente onde o contorno se fecha.

**A conta é sobre a tinta, não sobre a linha de centro.** As pontas do traço são redondas e avançam meia espessura além de cada extremidade, então cada traço do padrão sai uma espessura inteira mais longo do que a linha que o gerou. Medindo pela linha de centro, um traço de 12 px saía com o dobro do comprimento pedido e a folga quase fechada — quanto mais grosso o traço, mais o tracejado parecia sólido.

**O epaint termina um caminho aberto reto; o rasterizador da exportação o termina redondo.** Num traço sólido isso era meia espessura em cada ponta e passava despercebido; num tracejado seria a diferença entre a folga que se vê e a que se salva. O preview passou a fechar cada pedaço com um disco em cada extremidade, e com isso o traço sólido também ficou fiel ao arquivo.

**As partes vão juntas para o rasterizador**, e não uma por vez: a cobertura das duas é acumulada na mesma máscara e composta uma única vez. Compondo pedaço a pedaço, um rabisco de marca-texto que passa por cima de si mesmo receberia a cor duas vezes e o cruzamento sairia mais escuro — o mesmo motivo pelo qual o traço contínuo já usava máscara.

**A quebra mora fora do rasterizador** (`editor/dash.rs`), e não dentro dele: preview e exportação precisam percorrer exatamente os mesmos sub-caminhos, ou o JPG sairia diferente do que estava na tela. O `egui::Shape::dashed_line` ficou de fora pelo mesmo motivo — ele calcula os traços por conta própria, com pontas retas, enquanto o rasterizador dá pontas redondas.

**O pontilhado devolve sub-caminhos de um ponto só.** Tanto o `stroke_polyline` quanto o preview já tratavam um ponto solitário como a marca redonda da ponta; o ponto do padrão não precisou de caminho próprio.

Retângulo e elipse são anéis implícitos no rasterizador — testes de região, sem ordem nem comprimento de arco —, então o contorno deles vira polilinha amostrada **só** quando o padrão não é sólido. O caso comum continua no caminho rápido, com o anti-aliasing que já tinha.
