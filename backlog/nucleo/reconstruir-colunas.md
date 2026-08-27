# Reconstruir colunas no OCR

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** G
**Depende de:** OCR disponível na plataforma

## O que é

Preserva o alinhamento de tabelas no texto reconhecido.

## Como fazer

Projetar os `BoundingRect` das palavras numa grade, achar as faixas vazias e
usar os pontos médios como divisórias. O PowerToys resolve assim em
`Models/ResultTable.cs`; ver `docs/ocr-viabilidade.md`, seção 5.3.
