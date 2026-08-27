# Redimensionar a captura

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** P

## O que é

Escalar a imagem inteira dentro do editor.

## Como fazer

Operação `Scale(fator)` no op-log, aplicada no replay depois da imagem-fonte;
as anotações acompanham multiplicando as coordenadas. A reamostragem
bilinear já existe em `platform/ocr.rs` e deve migrar para o `imgbuf`.
