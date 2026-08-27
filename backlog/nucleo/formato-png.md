# Saída em PNG

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** M

## O que é

Hoje tudo sai em JPG q90, que borra bordas de texto e interface — o conteúdo
mais comum de uma captura de tela.

## Como fazer

Codificador PNG: filtros por linha e `deflate` (já na árvore por outras
crates). Vale junto a escolha automática, como o Shottr faz: PNG quando a
imagem tem poucas cores e bordas duras, JPG quando é fotográfica.

## Como ficou

Entregue em 27/08/2026, em `src/imgout.rs`.

**A crate `png` não é dependência nova** — já entrava por
`eframe -> image -> png` (e por `arboard`, pelo mesmo caminho). Mesmo
argumento que valeu para a crate `windows` do OCR. O JPEG continua
vendorizado porque cabe em três arquivos; o PNG não caberia, já que o formato
exige `deflate`, e um deflate próprio seriam ~500 linhas comprimindo pior.

**A escolha automática** conta cores distintas numa amostra de até 4096
pixels: acima de 25% de cores únicas a imagem é tratada como fotográfica e
sai em JPG; abaixo disso, PNG. Interface e texto repetem cor em áreas
grandes e ficam bem abaixo do limiar; gradientes e fotos quase não repetem.

A amostragem anda com passo derivado do total, não coluna a coluna: amostrar
só uma faixa vertical diria que a tela inteira tem duas cores.

**A extensão é resolvida antes de reservar o caminho.** Reservar `.jpg` e
gravar PNG dentro deixaria o arquivo mentindo sobre si.

O campo `image_format` do `config.json` voltou a valer — ele existia na v1.0,
ficou ignorado da v1.1 à v1.8, e agora aceita `auto` (padrão), `png` e `jpg`.
Também há um seletor na janela de Configurações.
