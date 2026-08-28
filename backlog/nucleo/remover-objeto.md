# Remover objeto

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** G

## O que é

Apaga um elemento e preenche o buraco com o que estaria atrás.

## Como fazer

É inpainting. Para captura de tela o caso comum é fundo liso ou padrão
repetido, e propagação de cor a partir da borda já resolve a maioria. Sobre
foto o resultado fica ruim, e é aceitável.

## Como ficou

Entregue em 28/08/2026: ferramenta **Remover objeto** (`K`), ao lado de Ocultar e Holofote. Arraste sobre o elemento e ele some, com o fundo reconstruído a partir da borda.

**Equação de Laplace com a borda como condição de contorno.** O buraco começa preenchido por uma média das quatro arestas ponderada pelo inverso da distância — que num fundo chapado já é a resposta exata — e depois passa por 24 alisamentos que apagam a emenda entre as arestas. Um degradê volta degradê, e não um bloco chapado com a cor média.

**Sobre foto o remendo aparece, e isso é aceitável**: o alvo é a interface, onde o fundo por trás de um elemento é quase sempre liso ou uma rampa suave. A dica da ferramenta diz o que ela faz — "reconstrói o fundo a partir da borda" — em vez de prometer mágica.

**É queimado na imagem, como a redação**, e pelo mesmo caminho: o replay do documento aplica os remendos antes das redações — o remendo reconstrói fundo, e o que for censurado depois não pode ser reconstruído a partir dele. Desfazer devolve o elemento porque a imagem é reconstruída do log, e não remendada de volta.

**Um retângulo colado na borda da imagem não quebra**: onde falta vizinho de um lado, vale o do outro.
