# Saída em PNG

**Plataforma:** nucleo · **Estado:** parcial · **Esforço:** M

## O que é

Hoje tudo sai em JPG q90, que borra bordas de texto e interface — o conteúdo
mais comum de uma captura de tela.

## Como fazer

Codificador PNG: filtros por linha e `deflate` (já na árvore por outras
crates). Vale junto a escolha automática, como o Shottr faz: PNG quando a
imagem tem poucas cores e bordas duras, JPG quando é fotográfica.
