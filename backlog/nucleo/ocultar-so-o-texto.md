# Ocultar só o texto

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** M
**Depende de:** OCR disponível na plataforma

## O que é

Borra apenas as letras da região, preservando gráficos e layout.

## Como fazer

O `OcrResult` traz `BoundingRect` por palavra: reconhecer, coletar as caixas
e aplicar a redação existente só nelas, com folga de alguns pixels.

## Notas

Onde o OCR não reconhece, nada é ocultado — texto em fonte incomum passaria
intacto. A interface precisa deixar claro que é melhor-esforço, senão a
funcionalidade promete sigilo que não entrega.

## Como ficou

Entregue em 28/08/2026: com a ferramenta **Ocultar** (`D`) ativa, um botão da barra liga o modo "só as palavras". Arrastar uma região passa a reconhecer o texto dela e apagar uma caixa por palavra, em vez de tapar o retângulo inteiro — gráficos, ícones e o layout continuam visíveis.

**A ressalva mora na dica do botão**, não numa nota de rodapé: "é melhor-esforço: o que o reconhecimento não achar continua visível". E o aviso do fim diz **quantas palavras** foram apagadas, para o usuário conferir em vez de confiar. Uma funcionalidade que promete sigilo e entrega quase-sigilo é pior que não existir.

**O OCR bloqueia**, então o retângulo arrastado não vira anotação na hora: ele fica esperando numa bandeira da sessão, e quem o converte é o `app`, na mesma thread de trabalho por onde o botão de reconhecer texto já passava. Se o editor fechar enquanto o motor trabalha, o resultado é descartado.

**Todas as palavras entram como uma operação só**, reaproveitando o `AnnotateMany` que a colagem de anotações trouxe: desfazer devolve a região inteira, e não palavra por palavra. Cada caixa ganha semente própria de mosaico, porque quem as cria é o mesmo `paste` da colagem.

**Folga de 2 px em volta de cada palavra.** A caixa que o motor devolve encosta nos glifos, e sem folga sobram fiapos de letra nas bordas — um fiapo de letra ainda é informação.

As caixas voltam do motor divididas pela ampliação de 1,5× que ele já aplicava: quem chama pensa na imagem que entregou, não na que o WinRT viu.
