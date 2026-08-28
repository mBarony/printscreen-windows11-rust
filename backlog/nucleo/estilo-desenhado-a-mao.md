# Estilo desenhado à mão

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** M

## O que é

Variante rabiscada para seta, formas e texto — o traço sai irregular.

## Como fazer

Perturbar os pontos antes de rasterizar, com deslocamento pseudoaleatório ao
longo do contorno e duas passadas levemente diferentes.

A semente fica guardada na camada, como já acontece na redação: sem isso o
traço mudaria a cada quadro e a anotação tremeria na tela.

## Como ficou

Entregue em 28/08/2026: um botão na barra, ao lado do padrão de traço, que liga o tremido para linha, seta, retângulo, elipse, mão livre, marca-texto e régua. Duas passadas levemente diferentes por cima do mesmo caminho — é a segunda que dá o aspecto de quem repassou o traço para reforçá-lo.

**A semente não precisou de campo novo: é o `id` da camada.** Ele já é estável entre quadros e entre sessões, e a cópia já nasce com um id novo — que é exatamente a propriedade que a redação obtém renovando a semente à mão ao duplicar. Duas anotações iguais não saem com o mesmo tremido, e a mesma anotação não treme a cada quadro.

**A perturbação é função do comprimento percorrido, não da posição na imagem.** Se dependesse da posição, arrastar a anotação a redesenharia a cada pixel do arrasto.

**Todos os pontos originais sobrevivem; só os segmentos longos ganham pontos no meio.** A primeira versão reamostrava o caminho inteiro num passo fixo, o que era mais curto e achatava as curvas: a elipse, que já vem amostrada em 64 pontos, voltava como um polígono de uma dúzia de lados. Subdividir preserva a curva e ainda assim treme uma reta de duas pontas.

**O desvio é interpolado entre nós, com smoothstep.** Sorteado ponto a ponto o traço vira serrilha, não tremida; com interpolação linear ficaria um bico em cada nó.

**As duas pontas ficam presas** por uma envoltória que zera nos extremos: uma seta cuja ponta sai do alvo deixa de apontar para o que foi apontado, e um retângulo com o canto solto deixa de fechar.

Ficou de fora, de propósito: **o texto** (exigiria uma segunda fonte, e o executável carrega só a Inter Regular), **a ponta da seta e o aro do numerador** (são silhuetas, não traços — tremidas viram borrão) e **o preenchimento** das formas cheias, que continua chapado em vez de hachurado.
