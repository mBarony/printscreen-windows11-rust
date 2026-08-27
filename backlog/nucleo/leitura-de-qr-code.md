# Leitura de QR code

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** M

## O que é

Decodifica o QR da seleção e copia o conteúdo.

## Como fazer

Algoritmo fechado e bem documentado: localizar os três padrões de canto,
corrigir a perspectiva, amostrar a grade, aplicar a máscara, decodificar
Reed-Solomon. Sem dependência nova em nenhuma plataforma.

## Notas

Melhor relação valor/esforço da lista: distintivo e delimitado.
