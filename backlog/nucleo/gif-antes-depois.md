# GIF antes e depois

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** M

## O que é

Duas capturas viram um GIF de dois quadros que alterna entre elas.

## Como fazer

Codificador GIF: quantização para 256 cores e LZW, ~300 linhas, no espírito
do JPEG já vendorizado. A quantização é o ponto delicado — gradientes de
interface ficam com faixas visíveis numa paleta pequena.

## Como ficou

Entregue em 28/08/2026: um botão na barra do editor salva um GIF de dois quadros que alterna, a cada segundo, entre a captura **sem** as anotações e a captura **com** elas.

**Os dois quadros são o antes e o depois do trabalho feito no editor**, e não duas capturas soltas escolhidas num diálogo. Os dois saem do mesmo `content_image`, então têm o mesmo tamanho de graça — mesmo depois de um recorte — e, como esse é o conteúdo já redigido, o GIF nunca revela o que foi ocultado.

**O botão não fecha o editor.** É um artefato a mais, não o fim da tarefa: quem pede o GIF quase sempre ainda quer salvar a imagem parada depois.

**A paleta é única para os dois quadros.** Com uma paleta por quadro, as mesmas cores cairiam em índices diferentes e o GIF piscaria de cor a cada troca — exatamente o que um "antes e depois" não pode fazer.

**Corte mediano, e não histograma popular.** Uma seta vermelha ocupa uma fração de 1% dos pixels; num histograma por popularidade ela sumiria da paleta, e é justamente ela que o GIF existe para mostrar.

**Pontilhado ordenado (Bayer 4×4) contra o bandeamento** que o backlog previa nos gradientes. Ordenado, e não por difusão de erro: o padrão é o mesmo nos dois quadros, então a área que não mudou entre um e outro sai **idêntica** e o GIF não ferve.

**Cache de cor de 15 bits no mapeamento.** Sem ele, cada pixel custaria uma varredura das 256 cores da paleta: numa captura 4K seriam dois bilhões de comparações por quadro.

O LZW é testado por um **descompressor escrito nos testes**: um codificador que ninguém decodifica é um gerador de bytes bonitos. O round-trip cobre área chapada, sequência longa sem repetição, o caso KwKwK e um byte só.
