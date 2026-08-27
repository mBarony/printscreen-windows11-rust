# Ocultar só o texto

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** M
**Depende de:** OCR disponível na plataforma

## O que é

Borra apenas as letras da região, preservando gráficos e layout.

## Como fazer

O `OcrResult` traz `BoundingRect` por palavra: reconhecer, coletar as caixas
e aplicar a redação existente só nelas, com folga de alguns pixels.

## Notas

Onde o OCR não reconhece, nada é ocultado — texto em fonte incomum passaria
intacto. A interface precisa deixar claro que é melhor-esforço, senão a
funcionalidade promete sigilo que não entrega.
