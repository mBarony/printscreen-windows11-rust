# Leitura de QR code

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** G

## O que é

Decodifica o QR da seleção e copia o conteúdo.

## Como fazer

Algoritmo fechado e bem documentado: localizar os três padrões de canto,
corrigir a perspectiva, amostrar a grade, aplicar a máscara, decodificar
Reed-Solomon. Sem dependência nova em nenhuma plataforma.

## Notas

Melhor relação valor/esforço da lista: distintivo e delimitado.

## Reestimativa (27/08/2026)

Subiu de **M para G** ao ser atacado. O decodificador em si é o que o backlog
descrevia, mas falta a metade que não estava à vista: **não há como testá-lo**.

Não existe crate de QR na árvore nem ferramenta de geração no ambiente, então
não há QR de entrada para exercitar o decodificador. As saídas são:

- escrever também um **gerador** de matriz QR só para os testes — praticamente
  dobra o trabalho;
- embutir matrizes de QR conhecidas como dados de teste — mas elas teriam de
  vir de algum lugar confiável, e transcrever à mão não é;
- entregar sem teste — inaceitável para este tipo de código, que ou decodifica
  exatamente ou não decodifica. Reed-Solomon errado por um bit devolve lixo
  silenciosamente.

O caminho recomendado é o primeiro: encoder mínimo (sem correção de erro além
do necessário) para gerar entrada, e o decodificador completo por cima. Some
Reed-Solomon (~150 linhas), detecção dos padrões de canto, correção de
perspectiva, leitura em zigzag e os modos numérico/alfanumérico/byte.
