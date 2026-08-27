# Colar imagem sobre a captura

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** M

## O que é

Sobrepõe outra imagem como camada movível.

## Como fazer

Novo `Shape::Image { rect, pixels }`. O peso é a persistência: o op-log é
JSON, e uma imagem embutida cresce demais — guardar os pixels ao lado,
referenciados por id, como o documento de trabalho já faz.
