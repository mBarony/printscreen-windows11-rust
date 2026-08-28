# Copiar e colar anotações

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** M

## O que é

Copiar anotações selecionadas e colá-las noutra captura.

## Como fazer

Serializar as `Layer` com o `json.rs` num formato próprio na área de
transferência. Ao colar, reconhecer o formato e gerar ids novos.

## Como ficou

Entregue em 28/08/2026: `Ctrl+Shift+C` copia as anotações selecionadas e `Ctrl+V` as cola — na mesma captura ou em outra, inclusive noutra janela do editor.

**Copiar é `Ctrl+Shift+C`, e não `Ctrl+C`.** No editor, `Ctrl+C` copia a imagem inteira e **fecha a janela**; fazer o significado dele depender de haver ou não seleção transformaria a ação de sair num sorteio. Colar ficou no `Ctrl+V` puro porque ali não havia disputa.

**Elas viajam como texto**, num JSON com a chave `rustshot_layers`, e não num formato de clipboard registrado: texto atravessa processos sem nada além do que o Windows já oferece, e é o mesmo caminho pelo qual o OCR já copia o que reconhece. Quem colar num editor de texto vê um JSON — feio, mas honesto. Qualquer outro texto na área de transferência é simplesmente ignorado ao colar.

**Colam nas mesmas coordenadas em que foram copiadas.** O uso que justifica a feature é anotar duas capturas do mesmo lugar da tela com as mesmas marcas; um deslocamento automático estragaria justamente isso. Como saem selecionadas, e a ferramenta vira a de mover, arrastá-las para outro lugar é um gesto só.

**Uma colagem é um passo de desfazer, não um por anotação.** Foi o motivo de existir a operação `AnnotateMany` no log: colar cinco anotações é um gesto do usuário, e desfazer tem de devolver o gesto inteiro.

Uma **redação colada ganha semente nova**, como a duplicada: dois mosaicos idênticos denunciariam que escondem a mesma coisa.
