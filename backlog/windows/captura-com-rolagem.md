# Captura com rolagem

**Plataforma:** windows · **Estado:** falta · **Esforço:** G

## O que é

Costura uma página mais alta que a tela numa imagem só.

## Como fazer

Não há API que role uma janela alheia de forma confiável:

1. Capturar o quadro visível.
2. Enviar `WM_MOUSEWHEEL` à janela sob o cursor e esperar assentar.
3. Achar a sobreposição entre quadros por correlação de faixas.
4. Emendar e repetir até o conteúdo parar de mudar.

A detecção precisa tolerar cabeçalhos fixos e rolagem suave, que produz
quadros intermediários borrados.

## Notas

A costura (passo 3) é o trabalho de verdade e é o que decide se funciona.
