# Redimensionar a captura

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

Escalar a imagem inteira dentro do editor.

## Como fazer

Operação `Scale(fator)` no op-log, aplicada no replay depois da imagem-fonte;
as anotações acompanham multiplicando as coordenadas. A reamostragem
bilinear já existe em `platform/ocr.rs` e deve migrar para o `imgbuf`.

## Como ficou

Entregue em 28/08/2026 como `Op::Scale(fator)` no op-log, com um campo de
porcentagem na barra do editor.

A reamostragem bilinear saiu de `platform/ocr.rs`, onde vivia só para o
motor, e virou `imgbuf::resized` — que é o lugar dela. O alinhamento é por
centro de pixel: sem isso a imagem escorrega meio pixel para o canto a cada
redimensionamento, o que aparece depois de dois ou três.

As anotações acompanham por `Shape::scale`, que multiplica **também os
raios** — uma elipse que escalasse só o centro viraria outra forma.

O campo volta sempre a 100% porque o fator é relativo ao tamanho atual;
mostrar um acumulado exigiria guardar o original só para isso.
