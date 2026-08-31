# Leitura de QR code

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** G

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

## Como ficou

Entregue em 31/08/2026, no mesmo comando do reconhecimento de texto (`Ctrl+Alt+PrtScr`) e **antes** dele: quem seleciona um QR quer o endereço que ele carrega, não o OCR dos quadradinhos. A tentativa é barata — sem os três padrões localizadores ela desiste em milissegundos — e o texto segue o caminho de sempre quando não há QR. Como é código próprio, sem WinRT, funciona também na build sem a feature `ocr`.

O módulo `src/qr/` se divide em duas metades separadas por um tipo: `detecta` vai da imagem à `Grade` de módulos, e `dados` vai da `Grade` ao texto. É essa fronteira que torna a segunda metade testável sem imagem nenhuma.

**A reestimativa acertou no diagnóstico e errou na conclusão.** O gerador de teste foi escrito (`gera`, só em `cfg(test)`), mas ele sozinho não prova nada: gerador e decodificador escritos juntos erram juntos, e o teste de ida-e-volta passa do mesmo jeito. O que quebrou o ciclo foram três coisas que vieram de fora:

1. **Cinco símbolos de referência publicados** (`referencia.rs`): as figuras do Anexo I.2, da Figura 1 e da Figura 29 do ISO/IEC 18004:2015(E), pelas matrizes de referência da biblioteca segno, mais dois literais de teste do zxing. Entre eles cobrem os três modos de segmento, um símbolo de versão 4 com dois blocos intercalados e padrão de alinhamento, e um cujo formato desmascarado é exatamente zero.
2. **Derivar em vez de transcrever.** As 32 palavras de informação de formato saem do BCH(15,5) que as define, as posições dos padrões de alinhamento saem da progressão que gera a tabela E.1, e o total de codewords sai da contagem de módulos de função. Sobrou transcrito só o que o padrão não deriva — duas linhas por nível da Tabela 9 —, e a divisão em grupos vem daí por aritmética.
3. **Invariantes que cruzam as tabelas entre si.** As 160 combinações de versão e nível têm de fechar com o total de codewords derivado, e a contagem de módulos que não são função tem de bater com esse mesmo total nas 40 versões. Uma transcrição errada aparece como número que não fecha, não como texto errado em produção.

Trinta e dois testes, e os que valem são os de fora: se os cinco símbolos do ISO e do zxing decodificam, duas implementações independentes concordam sobre o mesmo símbolo.

O que ficou de fora, de propósito: modo kanji (precisaria da tabela Shift-JIS inteira para um caso que não aparece numa captura de tela — é recusado, não lido pela metade) e correção de perspectiva por homografia. O mapeamento é afim a partir dos três localizadores, o que cobre rotação, escala e cisalhamento; falta só a perspectiva de câmera, que uma captura de tela não tem.
