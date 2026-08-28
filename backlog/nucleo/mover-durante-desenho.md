# Mover o objeto durante o desenho

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

Segurar espaço enquanto desenha reposiciona o que está sendo criado.

## Como fazer

Com a tecla ativa, somar o delta do ponteiro à origem em vez de ao ponto final.

## Como ficou

Entregue em 28/08/2026: segurar `Espaço` durante o desenho reposiciona a
forma em vez de esticá-la.

O deslocamento é somado ao ponto de partida, e os pontos já amostrados de um
traço à mão livre andam junto — senão o rabisco ficaria para trás do resto.
Enquanto o espaço estiver pressionado nenhuma amostra nova é coletada, senão
o movimento entraria no traço.

Errar o ponto de partida de um retângulo grande custava refazer o gesto
inteiro.
