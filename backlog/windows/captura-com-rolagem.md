# Captura com rolagem

**Plataforma:** windows · **Estado:** feito · **Esforço:** G

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

## Como ficou

Entregue em 28/08/2026: **Capturar com rolagem** no menu da bandeja. Aponte o cursor para a janela que deve rolar, escolha o item, e o RustShot rola sozinho até a página acabar e salva a página inteira numa imagem.

**A faixa de referência sai do meio do quadro, não do topo.** É a decisão que faz a coisa funcionar: cabeçalho e barra de status ficam parados enquanto o resto rola, e uma faixa tirada de lá casaria em deslocamento zero — a costura morreria no primeiro quadro achando que a página não andou. O teste `um_cabecalho_fixo_nao_engana_a_medida` guarda isso.

**Um quadro que não casa em deslocamento nenhum é recusado, não emendado.** Rolagem suave produz quadros intermediários borrados; costurá-los emendaria o borrão no meio da página. O piso de diferença média é o que os separa de um deslocamento legítimo.

**A roda vai para o controle mais fundo sob o ponto**, e não para a janela de topo: numa janela com painéis, quem rola é o controle sob o cursor, e o de topo costuma ignorar a roda.

**Roda na thread da bandeja, com pausas.** Ela fica travada por alguns segundos, e isso é aceitável: a captura precisa da GDI dessa thread, e o programa que rola tem a fila de mensagens dele, que continua andando. Um aviso avisa antes, outro conta a altura no fim.

**Teto de 40 passos.** Uma página que não termina — uma linha do tempo infinita — não pode prender a bandeja para sempre.

## O que não foi verificado

A **costura** tem sete testes sobre páginas sintéticas, incluindo cabeçalho fixo, quadro borrado e fim de página; ela é lógica pura e roda sem Windows.

O **acionamento** — `WM_MOUSEWHEEL` para o controle sob o cursor, e a espera de 320 ms até assentar — não foi exercitado contra um aplicativo real: a sessão em que isto foi escrito estava bloqueada. Os números de `SCROLL_NOTCHES` e `SCROLL_SETTLE_MS` são um ponto de partida razoável, não medidos. Vale testar contra um navegador e um editor antes de confiar.
