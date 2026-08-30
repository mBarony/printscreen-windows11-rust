# Arrastar para outro aplicativo

**Plataforma:** windows · **Estado:** feito · **Esforço:** M

## O que é

Tirar a captura do editor arrastando direto para outro programa.

## Como fazer

Implementar `IDropSource` e `IDataObject` (COM) e chamar `DoDragDrop`. É a
única parte que precisaria de vtables COM à mão sobre o `windows-sys` — ou
usar a crate `windows`, que já está no binário por causa do OCR.

## Como ficou

Entregue em 30/08/2026: um botão na barra do editor de onde se arrasta a captura anotada para qualquer lugar que aceite arquivo — Explorer, campo de anexo, área de mensagem.

As vtables são montadas à mão sobre o `windows-sys`, e não pela macro `implement` da crate `windows`: ela está atrás da feature `ocr`, e amarrar arrastar-e-soltar ao reconhecimento de texto pagaria caro por conveniência de escrita.

O que viaja é um **arquivo** (`CF_HDROP`), e não a imagem em memória: é o formato que os destinos aceitam, e um bitmap serviria a menos programas exigindo mais código. O PNG é gravado antes na pasta temporária, com o nome do template configurado — é esse nome que aparece no destino, e ele diz mais que `captura.png`. O temporário não é apagado depois: um destino que só lê o arquivo quando o usuário confirma encontraria o vazio.

O botão responde a `drag_started` e não a `clicked`, porque o `DoDragDrop` só rastreia o gesto se o botão do mouse já estiver pressionado quando ele começa; e a chamada **bloqueia** o editor enquanto o arrasto dura, que é o laço modal do OLE e o mesmo comportamento de qualquer aplicativo nativo.

Os dois objetos COM vivem na pilha da função durante a chamada inteira, que é síncrona, então o `Release` deles só decrementa a contagem — o que elimina por construção a liberação dupla, a classe de erro mais perigosa deste código.
