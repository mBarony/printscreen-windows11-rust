# Colar imagem sobre a captura

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** M

## O que é

Sobrepõe outra imagem como camada movível.

## Como fazer

Novo `Shape::Image { rect, pixels }`. O peso é a persistência: o op-log é
JSON, e uma imagem embutida cresce demais — guardar os pixels ao lado,
referenciados por id, como o documento de trabalho já faz.

## Como ficou

Entregue em 28/08/2026: `Ctrl+V` no editor cola a imagem da área de transferência como uma anotação movível, que se arrasta, redimensiona pelas alças, duplica e desfaz como qualquer outra.

**`Ctrl+V` é uma tecla só para dois formatos.** Se a área de transferência tiver anotações copiadas do próprio RustShot, elas são coladas; senão, se houver uma imagem, é ela. Quem cola não precisa saber o que está lá.

**Os pixels ficam num depósito do documento, referenciados por `source`** — a forma guarda o retângulo e o número, não os bytes. O log de operações é JSON e clona camadas a cada edição: uma imagem embutida cresceria o arquivo em megabytes e seria copiada inteira a cada arrasto.

**Na sessão gravada, um arquivo `pasted-N.rsraw` por imagem**, ao lado do log. São gravados antes do log — recuperar uma sessão cujo log aponta para pixels que não estão no disco deixaria buracos na tela — e só uma vez, porque uma imagem colada não muda depois. Ao recuperar, o próximo `source` continua de onde os recuperados pararam: reaproveitar um id trocaria os pixels de uma imagem que já está na tela.

**A imagem nasce centrada e cabendo na captura.** Colada em tamanho natural, uma imagem maior que a captura apareceria como uma parede de pixels sem cantos visíveis para arrastar.

**Um `source` sem pixels não derruba a exportação.** É o caso da sessão recuperada sem o arquivo: sai um buraco, e o resto do trabalho é salvo.

No preview, cada imagem ganha uma textura montada sob demanda pelo canvas — o documento é a fonte da verdade e não conhece GPU nenhuma. Na exportação, o rasterizador ganhou `fill_image`, com amostragem bilinear: uma imagem colada e esticada é justamente o caso em que o vizinho mais próximo denunciaria a escada.
