# Agrupamento a cada 5 botões na barra

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** P

## O que é

A barra do editor ganha um pequeno agrupamento a cada 5 botões, para o olho
achar um ícone contando em vez de varrendo.

## Como fazer

As duas peças já existem em `editor/ui/toolbar.rs`: `group` (linha 139), que
mantém um punhado de controles junto e sem quebrar no meio quando a linha
envolve, e `group_divider` (linha 148), o traço fino entre grupos. O trabalho é
a cadência, não o desenho.

## A decisão que precisa ser tomada antes

A barra **já é agrupada**, e por significado: ferramentas, opções do que está
selecionado, imagem inteira, e à direita saída e histórico — estes dois
separados de propósito, porque um clique errado entre fechar e desfazer custa
caro dos dois lados.

Uma cadência de 5 aplicada por cima, ignorando esses blocos, brigaria com eles:
o divisor cairia no meio de "saída" e emendaria o último botão de um grupo com
o primeiro do seguinte, desfazendo justamente a separação que existe para
evitar o clique errado.

A saída que preserva as duas coisas é aplicar a cadência **dentro** de cada
grupo, subdividindo só os que passam de 5. Hoje isso significa apenas o bloco
de ferramentas, que tem 16 — os outros já nascem abaixo do teto. A subdivisão
interna usaria um espaçamento maior em vez de um traço, para não competir com
o divisor que separa os grupos de verdade.

Vale confirmar com quem pediu se é isso: "a cada 5" pode ter sido descrição do
efeito desejado (achar o ícone mais rápido) e não da regra literal.

## Notas

Medir antes de decidir: a barra tem 21 chamadas de `icon_button`, mas nem todas
aparecem ao mesmo tempo — parte é condicional ao que está selecionado. Uma
cadência calculada sobre a lista estática não corresponde ao que o usuário vê.
