# Reconhecimento de texto

**Plataforma:** linux · **Estado:** aberto · **Esforço:** G

## O que é

**O único ponto em que a paridade não fecha sozinha.** O Linux não tem motor
de OCR do sistema, ao contrário do Windows (`Windows.Media.Ocr`) e do macOS
(Vision).

## Como fazer

Três caminhos, todos com custo:

| Caminho | Custo |
|---|---|
| Tesseract como dependência de runtime | ~25 MB, instalação separada — contraria a proposta de binário único |
| Vendorizar um motor leve | trabalho grande e qualidade menor |
| Linux sem OCR | a promessa de "mesmas funcionalidades" não se cumpre aqui |

É decisão de produto, não técnica.

## Notas

Bloqueia `nucleo/ocultar-so-o-texto` e `nucleo/reconstruir-colunas` nesta plataforma.
