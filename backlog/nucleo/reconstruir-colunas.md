# Reconstruir colunas no OCR

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** G
**Depende de:** OCR disponível na plataforma

## O que é

Preserva o alinhamento de tabelas no texto reconhecido.

## Como fazer

Projetar os `BoundingRect` das palavras numa grade, achar as faixas vazias e
usar os pontos médios como divisórias. O PowerToys resolve assim em
`Models/ResultTable.cs`; ver `docs/ocr-viabilidade.md`, seção 5.3.

## Como ficou

Entregue em 28/08/2026, em `src/ocr_layout.rs`: o texto reconhecido passa a ser montado a partir de **onde as palavras estão**, e não só da ordem em que o motor as devolveu. Quando o conteúdo é tabular, as colunas saem separadas por tabulação — colar numa planilha põe cada coluna na sua célula.

**As divisórias saem das faixas vazias.** As caixas das palavras são projetadas no eixo x, e as faixas por onde nenhuma palavra passa viram candidatas; o meio de cada faixa larga o bastante é uma divisória.

**"Larga o bastante" é medido em alturas de palavra, não em pixels.** O espaço entre duas palavras da mesma frase fica bem abaixo de 1,2 altura; o de uma coluna para a outra, bem acima. Medir em px faria o critério valer numa captura 1:1 e falhar numa a 300%.

**Só vira tabela quando ao menos duas linhas atravessam mais de uma coluna.** Um parágrafo comum tem faixas vazias por acaso — entre duas linhas curtas, por exemplo —, e encher de tabulações um texto corrido seria estragar o caso comum para atender o raro. Sem isso, o texto sai como sempre saiu: uma linha por linha.

**Célula vazia vira coluna vazia**, e não coluna omitida: sem o lugar guardado, uma linha à qual falta a primeira célula empurraria a segunda para a coluna errada ao colar na planilha.

O módulo é lógica pura sobre caixas — testa-se sem OCR, sem Windows e sem GPU, e é o que os sete testes fazem. O `platform::ocr` ganhou `recognize_words`, que devolve as palavras agrupadas nas linhas em que o motor as viu; `recognize` passou a ser ele mais a montagem, e `recognize_boxes` (da redação por palavra) é ele achatado.
