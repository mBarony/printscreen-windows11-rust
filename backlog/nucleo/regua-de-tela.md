# Régua de tela

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** M

## O que é

Mede distâncias em pixels, com setas nas pontas e o valor no meio.

## Como fazer

Nova `Shape::Ruler { a, b }`. A geometria é uma linha; o rótulo reaproveita
o badge que o overlay já usa para as dimensões da seleção. Medir em pixels
da imagem, não em pontos: a 150% de escala os números diferem.

## Como ficou

Entregue em 28/08/2026: ferramenta **Régua** (`U`), que arrasta um traço com uma ponta em cada extremidade e o valor medido numa pílula no meio. `Shift` prende em 45°, as duas pontas são alças e o traço se move, duplica e desfaz como qualquer outra anotação.

**A medida é em px da imagem, e por construção.** `a` e `b` já vivem no espaço da imagem, e o número sai de um `hypot` sobre eles antes de qualquer conversão para tela — a 150% de escala, ou com o editor em zoom, o valor não muda. Uma régua que medisse pontos de tela mediria a janela, não a captura.

**Sobrou uma linha: `ruler::geometry` é uma seta em cada sentido**, reaproveitando o triângulo e o recuo da haste que a seta já tinha. Cada ponta é limitada a metade do comprimento: com o teto da seta (10 px), uma régua de 12 px teria as duas pontas se atravessando e a haste do avesso.

**O badge do overlay não deu para reaproveitar literalmente.** Ele é `egui::Painter` puro, com `FontId::proportional(13.0)` e clamp à viewport. A exportação rasteriza com `ab_glyph`, sem egui; e `proportional` resolve para a Segoe UI onde ela existe — o rótulo sairia diferente do preview, e diferente por máquina. O que ficou é a pílula do texto (`text_pill_metrics`) na cor do traço, com a Inter embutida, igual nos dois caminhos. A tinta do número é branca ou quase-preta, a que tiver mais contraste APCA com a cor escolhida: numa régua branca o valor sumiria.

**A pílula tapa o miolo da haste de propósito.** O número é o que a régua tem a dizer, e um traço passando por trás dele briga com a leitura.
