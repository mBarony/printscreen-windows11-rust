# Semitransparência

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P
**Depende de:** formato-png (já entregue)

## O que é

Deixar a imagem translúcida, para sobrepor a outra coisa.

## Como fazer

Fator de alfa na exportação.

## Como ficou

Entregue em 28/08/2026: campo de opacidade (10–100%) na barra do editor,
aplicado ao arquivo salvo.

Abaixo de 100% a saída é **forçada a PNG**, mesmo que a preferência do
usuário seja JPG ou automático: o JPG não tem canal alfa, e gravar uma
imagem translúcida nele a devolveria opaca — o pedido sairia silenciosamente
ignorado.

Vale para o arquivo, não para a área de transferência: o `CF_DIB` do Windows
não carrega alfa de forma confiável entre aplicativos.
