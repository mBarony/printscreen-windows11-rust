# Changelog

Histórico de versões do RustShot. Datas em 2026.

## Não lançado

## v1.10.0 — 30/08

Duas mudanças, uma de cada lado da captura: a imagem anotada sai do editor arrastada direto para outro programa, e a seleção de região parou de piscar a tela ao abrir.

- **A tela não pisca mais ao abrir a seleção de região.** A janela do overlay nascia visível e vazia: o winit cria toda janela com `CW_USEDEFAULT` nas quatro coordenadas e só depois aplica tamanho e posição — e, pior, mostra a janela **antes** disso ("Set visible before setting the size", diz o comentário dele). O resultado era um retângulo branco de meia tela, com barra de título e botão de fechar de verdade (janela sem decoração no Windows nasce com `WS_CAPTION` e só finge não ter, por `WM_NCCALCSIZE`), que saltava, crescia até cobrir o monitor e só então pintava a captura congelada. Medido no vídeo do relato: 420 ms de retângulo branco. Agora o overlay nasce oculto e se exibe sozinho no primeiro quadro em que já não pede tamanho novo — os comandos de viewport valem na ordem em que são enviados, então quando o `Visible(true)` chega a geometria já foi corrigida e o conteúdo já foi pintado. A condição não é "a geometria está certa": janela oculta não recebe `WM_PAINT`, e no Windows é dele que sai todo `RedrawRequested`, então esperar por um quadro que ninguém vai pedir deixaria o overlay invisível para sempre. O que ainda acorda uma janela oculta é o `WM_SIZE`, e é por isso que a espera é atrelada ao pedido de tamanho, com teto de quatro quadros para o caso de uma geometria que não convirja.

- **O overlay não fica mais um segundo sem `WS_EX_LAYERED`.** Exibir a janela faz o winit reescrever o `GWL_EXSTYLE` inteiro a partir das flags dele, levando junto a camada que a raiz aplicou — e a raiz dorme enquanto a seleção roda, então ela só repunha o estilo mais de um segundo depois. Nesse intervalo o overlay é uma janela de tela cheia, topo-de-tudo e sem camada, que é exatamente a condição do apagão de monitor investigado na v1.1. A reaplicação passou a sair do quadro do próprio overlay, que é quem com certeza roda logo depois. O buraco já existia antes desta versão, em menor escala.

- **Arrastar a captura para outro programa**, por um botão da barra do editor: segure nele e arraste para o Explorer, para um campo de anexo ou para uma área de mensagem, e o que chega lá é a captura anotada como arquivo. O que viaja é um arquivo (`CF_HDROP`), e não a imagem em memória — é o formato que os destinos aceitam, e um bitmap serviria a menos programas por mais código. O PNG é gravado antes na pasta temporária, com o nome do template configurado, porque é esse nome que aparece no destino. O botão responde ao início do arrasto e não ao clique: o `DoDragDrop` só rastreia o gesto se o botão do mouse já estiver pressionado quando ele começa. É a única parte do programa que monta vtables COM à mão, para não amarrar arrastar-e-soltar à feature do reconhecimento de texto.

## v1.9.0 — 28/08

Uma leva grande de anotação e de captura. O editor ganhou padrão de traço,
traço desenhado à mão, régua, copiar-e-colar de anotações, colagem de imagem,
remoção de objeto e GIF antes-e-depois; o OCR passou a preservar colunas e a
ocultar só as palavras; e a captura ganhou seleção por elemento, rolagem
automática, escolha de destino e uma API por URL.

- **O aviso do OCR some ao colar.** `Ctrl+V` em qualquer aplicativo fecha a
  janela na hora: colar é o fim natural da tarefa, e insistir no aviso depois
  disso é ruído sobre o que o usuário foi fazer. O aviso não tem foco nesse
  momento — quem recebe a tecla é a janela de destino —, então a detecção é
  por consulta ao estado do teclado no pulso de 100 ms que a janela já tinha.
  Não há hook instalado: nada é interceptado nem registrado, a colagem segue
  para o destino como sempre, e a consulta é a duas teclas específicas. O
  fechamento por tempo continua valendo para quem não colar.
- **A barra do editor ficou mais limpa e mais organizada.** Os botões passaram
  de 26 para 30 pontos, com o ícone proporcionalmente maior, e o realce do
  cursor entra por uma animação curta — numa fila de quinze, o fundo
  surgindo de estalo a cada pixel percorrido piscava. Os controles deixaram de
  ser uma fila só com traços soltos e viraram blocos: ferramentas, opções do
  que está selecionado e imagem inteira à esquerda; saída e histórico à
  direita, em blocos separados de propósito, porque um clique errado entre
  fechar e desfazer custa caro dos dois lados. A ferramenta ativa passou a ser
  marcada por um fundo discreto no lugar da cor de destaque cheia, que puxava
  o olho e apagava o desenho do próprio ícone.
- Uma prévia SVG da barra montada, gerável sob demanda por teste ignorado, no
  mesmo molde da que já existia para os ícones soltos: dá para julgar
  proporção e agrupamento sem GPU e sem Windows.
- **Captura com rolagem**, no menu da bandeja: aponte o cursor para a janela, escolha o item, e o RustShot rola sozinho até a página acabar e salva tudo numa imagem só. A costura descobre quanto a página andou correlacionando uma faixa do **meio** do quadro — do topo ela casaria com o cabeçalho fixo e a captura morreria no primeiro passo. Um quadro que não casa em deslocamento nenhum é recusado em vez de emendado: é o que a rolagem suave produz no meio do caminho.
- **Seleção inteligente** no overlay: `Espaço` passou a ciclar três modos — arrastar à mão, escolher uma janela inteira e escolher o **elemento sob o cursor**. A detecção é por imagem, e não por `EnumChildWindows`: o caminho nativo acertaria tudo no Bloco de Notas e nada no VS Code ou no navegador, que é onde as capturas acontecem. A cor da superfície é a dominante em volta do cursor, e não a de baixo dele — sobre uma letra, a de baixo é a tinta do glifo, e o que sairia era a letra. Uma superfície que atravessa o alcance da busca nos dois eixos é recusada: isso é o fundo da tela.
- **API por esquema de URL**: uma chave nas Configurações registra `rustshot://`, e a partir daí um link dispara o que a linha de comando já fazia — `rustshot://fullscreen?copy=1`, `rustshot://open?file=…`, `rustshot://clipboard` e `rustshot://ocr?file=…`. A URL vira o mesmo modo da CLI, com as mesmas regras, e só entram os comandos que rodam sem janela nem residente: região e edição dependem de um overlay sobre a tela. O registro fica no ramo do usuário, sem elevação.
- **Remover objeto** (`K`): arraste sobre um elemento e ele some, com o fundo reconstruído a partir da borda. É a equação de Laplace com a borda como contorno — um fundo chapado volta chapado e um degradê volta degradê, em vez de virar um bloco com a cor média. Sobre foto o remendo aparece, e a dica da ferramenta diz o que ela faz em vez de prometer mágica. É queimado na imagem como a redação, e desfazer devolve o elemento.
- **GIF antes e depois**, por um botão da barra do editor: dois quadros que alternam a cada segundo entre a captura sem anotações e a captura com elas. O codificador é próprio, como o JPEG — paleta única para os dois quadros (uma por quadro faria o GIF piscar de cor a cada troca), corte mediano em vez de histograma popular (uma seta vermelha ocupa menos de 1% dos pixels e é justamente ela que o GIF mostra) e pontilhado ordenado contra o bandeamento dos gradientes, com o mesmo padrão nos dois quadros para a área parada não ferver. O botão não fecha o editor: é um artefato a mais, não o fim da tarefa.
- **O texto reconhecido preserva as colunas de uma tabela.** Ele passou a ser montado a partir de onde as palavras estão: as faixas por onde nenhuma palavra passa viram divisórias, e as colunas saem separadas por tabulação — colar numa planilha põe cada coluna na sua célula. A largura mínima de uma faixa é medida em alturas de palavra, e não em pixels, senão o critério valeria a 100% e falharia a 300%. E só vale quando ao menos duas linhas atravessam mais de uma coluna: um parágrafo comum tem faixas vazias por acaso, e enchê-lo de tabulações seria estragar o caso comum para atender o raro.
- **Ocultar só as palavras de uma região**, preservando gráficos e layout: com a ferramenta Ocultar ativa, um botão da barra liga o modo, e o retângulo arrastado vira uma redação por palavra reconhecida. É melhor-esforço, e a interface diz isso em vez de esconder: a dica do botão avisa que o que o reconhecimento não achar continua visível, e o aviso do fim diz quantas palavras foram apagadas, para conferir em vez de confiar. Cada palavra leva 2 px de folga — a caixa do motor encosta nos glifos, e um fiapo de letra ainda é informação. Tudo entra como um passo de desfazer.
- **Colar uma imagem sobre a captura**, com `Ctrl+V`: ela vira uma anotação movível, que se arrasta, redimensiona pelas alças e desfaz como as outras. A mesma tecla serve aos dois formatos — se a área de transferência tiver anotações copiadas do RustShot, são elas; senão, a imagem. Os pixels ficam num depósito do documento, referenciados por número: o log de operações é JSON e clona camadas a cada edição, e uma imagem embutida cresceria o arquivo em megabytes. A imagem nasce centrada e cabendo na captura, senão colaria como uma parede de pixels sem cantos para arrastar.
- **Copiar e colar anotações entre capturas**: `Ctrl+Shift+C` copia as selecionadas e `Ctrl+V` as cola, na mesma imagem ou em outra. Copiar não é `Ctrl+C` porque no editor essa tecla copia a imagem e fecha a janela — fazer o significado dela depender de haver seleção transformaria a ação de sair num sorteio. As anotações viajam como texto num formato próprio, e colam nas mesmas coordenadas em que foram copiadas, que é o que serve para anotar duas capturas do mesmo lugar da tela; saem selecionadas, então arrastá-las é um gesto só. A colagem inteira é um passo de desfazer.
- **Traço desenhado à mão**, por um botão ao lado do padrão de traço: o desenho sai irregular, em duas passadas, para linha, seta, retângulo, elipse, mão livre, marca-texto e régua. A semente do tremido é o id da anotação — estável entre quadros e entre sessões, e diferente na cópia —, então nada treme na tela e duas anotações iguais não saem idênticas. As duas pontas ficam presas: uma seta cuja ponta sai do alvo deixa de apontar para o alvo. Texto, ponta de seta e preenchimento continuam firmes.
- **Escolher o que acontece depois de capturar**, em Configurações: salvar em arquivo, copiar para a área de transferência, ou os dois. A escolha é separada para a tela cheia e para a região, porque os dois fluxos nasceram com padrões diferentes de propósito — quem aciona a tela cheia quer o arquivo, quem recorta uma região quer colar — e esses padrões continuam valendo para quem não mexer em nada. Vale também para repetir a última região. A linha de comando não muda: `--capture-fullscreen` sem `--copy` nem `--save` continua salvando, como o `--help` promete.
- **Traço tracejado e pontilhado**, por um botão da barra que cicla os três padrões. Vale para linha, seta, retângulo, elipse, mão livre, marca-texto e régua, e repinta o que estiver selecionado como os outros controles de estilo. O padrão é medido em múltiplos da espessura — um tracejado de medida fixa sobre um traço de 12 px sairia como uma fila de quadrados colados — e sobre a tinta, não sobre a linha de centro: as pontas do traço são redondas e avançam meia espessura além de cada extremidade, então quanto mais grosso o traço, mais o tracejado pareceria sólido. O período é esticado para caber um número inteiro de vezes no caminho, de modo que ele começa e termina com tinta e a última esquina de um retângulo fecha.
- **Régua** (`U`): mede uma distância na captura, com uma ponta em cada extremidade e o valor no meio. A medida é em **pixels da imagem**, não em pontos de tela — a 150% de escala, ou com o editor em zoom, o número é o mesmo que o overlay mostra ao selecionar a região. `Shift` prende em 45°, as pontas são alças, e a régua se move, duplica e desfaz como qualquer outra anotação; ao redimensionar a captura ela se remede sozinha.
- **A seta pode ser dobrada num arco.** Selecione-a e arraste a alça do meio.
  A curvatura é proporcional ao comprimento, então setas de tamanhos
  diferentes ficam com o mesmo aspecto, e a ponta acompanha a tangente do fim
  da curva — apontá-la pela corda deixaria a farpa torta em relação ao traço
  que chega nela.
- **Guias de alinhamento** no editor: `Alt+H` e `Alt+V` criam uma linha de
  apoio onde o cursor está, `Alt+Shift+G` limpa todas. São só ajuda visual —
  não entram no histórico nem na imagem exportada.
- **Opacidade do arquivo salvo**, de 10% a 100%, por um campo na barra.
  Abaixo de 100% a saída vai em PNG mesmo que a preferência seja outra: o JPG
  não tem canal alfa e devolveria a imagem opaca, ignorando o pedido em
  silêncio.
- **Capturar tela cheia com 3 segundos de atraso**, pelo menu da bandeja —
  tempo de abrir um menu ou posicionar o cursor antes de a tela congelar.
- **Repetir a última região**, também pelo menu, sem passar pelo overlay. O
  retângulo é lembrado em coordenadas do desktop virtual, então continua
  válido mesmo que a lista de monitores mude; se ele não couber mais em
  nenhuma tela, um aviso explica em vez de capturar o lugar errado.
- **Redimensionar a captura inteira**, por um campo de porcentagem na barra
  do editor. As anotações acompanham, raios inclusive — uma elipse que
  escalasse só o centro viraria outra forma. É uma operação do histórico como
  as outras, então desfaz.
- **Desfazer os recortes** e voltar ao enquadramento original sem perder o
  resto do trabalho. Um recorte já consolidado pelo teto do histórico (100
  operações) não volta: ele deixou de ser reversível quando foi assado na
  imagem de partida.
- **`Shift` trava a seleção num quadrado** durante o arrasto no overlay,
  como o editor já fazia nas formas.
- **`Alt`+arrasto duplica a anotação** em vez de movê-la: a cópia nasce por
  cima e é ela que segue o ponteiro, deixando o original onde estava. É o
  `Alt+D` sem ter de reposicionar depois.
- **`Espaço` reposiciona a forma enquanto você a desenha**, em vez de
  esticá-la — errar o ponto de partida de um retângulo grande custava refazer
  o gesto inteiro.
- **`Alt+R` inverte a ponta da seta selecionada**, sem redesenhá-la.
- **O conta-gotas ganhou duas amostragens.** Um clique continua pegando o
  pixel exato. Com `Shift`, ele pega o **tom mais escuro** num quadrado de
  20×20 px em volta do cursor — que num texto é a cor da letra, e não a do
  fundo, que é o que o clique simples quase sempre pegava. Arrastando, ele
  pega a **média** do retângulo, para áreas com ruído ou gradiente onde um
  pixel só não representa o que se está olhando.
- **A cor também aparece em OKLCH e com contraste APCA**, na dica do botão de
  cor, ao lado do HEX. O OKLCH é perceptualmente uniforme, e o APCA responde
  direto "dá para ler texto nesta cor?" contra branco e contra preto.
- **Saída em PNG, com escolha automática de formato.** Até aqui tudo saía em
  JPG q90, que borra bordas de texto e interface — justamente o conteúdo mais
  comum de uma captura de tela. O padrão passa a ser `auto`: a decisão é por
  imagem, contando cores distintas numa amostra. Interface e texto repetem
  cor em áreas grandes e vão para PNG, que guarda tudo intacto; fotos e
  gradientes quase não repetem e vão para JPG, que poupa espaço onde a perda
  não se nota. Dá para forçar um dos dois pelo seletor em Configurações ou
  pelo `image_format` do `config.json` — campo que existia na v1.0, ficou
  ignorado da v1.1 até aqui, e volta a valer.
- **Fixar a captura na tela** (`Ctrl+P`, ou o botão de alfinete na barra do
  editor). A imagem vira uma janelinha sem bordas, sempre no topo, que fica
  até você fechá-la com `Esc` — para consultar algo enquanto trabalha noutra
  janela. Como não tem barra de título, o corpo inteiro é a área de arrasto, e
  a roda redimensiona entre 15% e 400%. Uma captura maior que 520 pontos nasce
  encolhida: em tamanho natural, uma tela cheia fixada cobriria o monitor.

## v1.8.0 — 27/08

- **Capturar região passou a copiar direto.** Soltar o arrasto põe a região
  na área de transferência e encerra, sem o passo de escolher entre `Ctrl+C`
  e `Ctrl+S`. A pergunta cobrava uma tecla a mais de todo mundo para servir
  ao caso raro: quem captura uma região quase sempre vai colar em seguida, e
  quem quer arquivo tem a captura de tela cheia e o editor. Saíram junto o
  estado pendente do overlay e o destino "salvar" da seleção; a dica na tela
  agora diz o que aquele arrasto vai fazer, que muda conforme o atalho usado.
- **O aviso do OCR deixou de ter o texto por cima do botão.** A prévia era
  desenhada primeiro e tomava a linha inteira, e o botão vinha por cima; além
  disso um reconhecimento de várias linhas crescia para baixo e passava por
  ele. Agora o botão é posicionado antes e a prévia fica com o espaço
  restante, cortada no fim da linha e limitada a duas linhas.
- **O botão desse aviso ficou maior**, com um alvo de clique de 108×28 pontos
  no lugar do `small_button` anterior. A janela acompanhou, de 400×68 para
  440×76 pontos.

## v1.7.1 — 24/08

- **A janela de captura passou a comprometer 61 MB a menos de memória.** O
  padrão do wgpu para `max_non_sampler_bindings` é 1.000.000, e no D3D12 esse
  número não é um teto: o backend cria um descriptor heap shader-visible com
  essa quantidade de descritores na criação do device, antes de existir um
  pixel. Este app usa entre cinco e nove. Baixar o limite para 4096 derrubou
  a memória privada do overlay de seleção de 105,7 MB para 44,4 MB, e a de
  qualquer janela em cerca de 60 MB. O working set quase não muda — o heap
  era memória comprometida e pouco residente —, mas é a comprometida que
  conta contra o limite de commit do sistema.

## v1.7.0 — 24/08

Port das funcionalidades do
[omasnap](https://github.com/tobi/omasnap), a ferramenta equivalente para
Arch Linux.

**Ferramentas novas**

- **Mão livre** (`F`) e **marca-texto** (`H`). O traço é suavizado por
  Béziers com pontos médios e amostrado com filtro de 1,5 px, para o gesto
  não tremer. O marca-texto é o mesmo traço 3× mais grosso e translúcido:
  ele marca sem esconder o que está embaixo.
- **Numerador** (`N`): um clique carimba o próximo número da sequência. Ela
  acompanha o que está na tela — apagar o de maior número devolve aquele
  número ao próximo, em vez de deixar buraco na sequência.
- **Conta-gotas** (`I`): toma a cor de um pixel da imagem e volta sozinho
  para a ferramenta anterior, preservando a seleção — amostrar uma cor é
  para aplicá-la em algo.
- **Retângulo e elipse preenchidos**, e cantos arredondados no retângulo.
  A forma cheia passa a ser agarrável pelo miolo; a vazada continua pegando
  só pelo contorno, porque o interior dela ainda é a imagem.
- **Texto multilinha** (`Ctrl+Enter` confirma, `Enter` insere linha) com uma
  **pílula clara** opcional atrás, para o texto continuar legível sobre
  qualquer fundo.

**Esconder, destacar, encurtar**

- **Ocultar** (`D`): apaga uma região de vez. O modo padrão é um mosaico
  **sintético** — as amostras servem só para descobrir os seis tons que
  dominam a região, as posições delas são descartadas, e cada bloco recebe um
  desses tons sorteado por um gerador semeado. Um pixelate por média, que é
  como quase todo mundo faz, preserva informação e sobre texto deixa um
  padrão que ataques de despixelização exploram; este não deixa. Há também o
  modo de cor chapada.
- **Holofote** (`O`): escurece o resto da imagem e amplia o que ficou dentro,
  com recorte em elipse, retângulo ou retângulo arredondado. Ele amostra a
  imagem **já ocultada**, então ampliar uma área censurada não revela nada.
- **Remover faixa** (`X`): joga fora uma tira da imagem e junta o que sobrou,
  para encurtar uma captura longa sem perder as pontas. As anotações
  acompanham; as que estavam dentro da faixa encostam na costura.
- **Fundos decorativos**: a captura sobre um degradê com sombra, em quatro
  variações, do jeito que uma imagem de tela costuma ser publicada.

**Captura**

- **Captura por janela**: `Space` no overlay alterna entre arrastar uma
  região e escolher uma janela; o ponteiro destaca a que está sob ele, as
  setas navegam e `Enter` captura. `Ctrl+A` pega o monitor inteiro. A posição
  do ponteiro em px aparece ao lado dele enquanto nada está selecionado.
- **O mesmo atalho dispensa o overlay**: acioná-lo com a seleção na tela
  fecha o overlay em vez de avisar que o app está ocupado. Com o editor
  aberto o aviso continua — ali há trabalho que um atalho não pode descartar.

**Abrir, recuperar, automatizar**

- **Abrir imagens existentes**: `rustshot <imagem>` (ou arrastar o arquivo
  sobre o executável) abre o editor direto sobre ela; `--clipboard` faz o
  mesmo com a imagem da área de transferência.
- **A edição sobrevive a um fechamento inesperado**: o editor grava a sessão
  enquanto se trabalha, e o menu da bandeja oferece recuperá-la — com o
  histórico de desfazer intacto, porque o que se grava é o log de operações e
  não a imagem achatada.
- **Linha de comando**: `--help`, `--version`, códigos de saída e
  `--capture-fullscreen [--copy] [--save]`, que captura e sai sem abrir
  janela.

**Edição e fundação**

- **Seleção múltipla**: arrastar a partir de um ponto vazio laça as
  anotações que couberem inteiramente dentro; elas passam a se mover e a ser
  apagadas em bloco, cada gesto como um único passo de desfazer.
- **Reconhecer texto** (`Ctrl+Alt+PrtScr`, ou o botão na barra do editor):
  a região selecionada — ou a imagem aberta no editor — vai ao motor de OCR
  do Windows e o texto cai na área de transferência, com as quebras de
  linha. Um aviso no alto da tela mostra o começo do que foi copiado e
  deixa recopiar tudo emendado numa linha só, para colar em campo único.
- **Selecionar tudo** (`Ctrl+A`) marca todas as anotações de uma vez, sem
  precisar cercá-las com o laço. Com `Delete` em seguida, limpar a captura
  inteira são dois toques.
- **Clicar no meio de uma forma vazada agora a seleciona**, quando não há
  nada mais sob o cursor. O contorno continua sendo o alvo preferido e o que
  estiver dentro da forma continua vencendo o clique — o miolo é o último
  recurso. Antes, clicar dentro de um retângulo não selecionava nada, e quem
  tentava apagá-lo por ali concluía que o editor não sabia apagar.
- **Anotações continuam editáveis depois de criadas.** A anotação
  selecionada ganha alças de redimensionamento — as oito da caixa nas formas
  com área, as duas pontas em linha e seta. `Shift` preserva a proporção num
  canto ou prende a ponta em 45°. As setas do teclado empurram 1 px (10 px
  com `Shift`), `Alt+D` duplica e `Delete` apaga. Trocar cor, espessura ou
  tamanho da fonte com algo selecionado repinta aquela anotação, em vez de
  valer apenas para a próxima — antes o estilo era congelado no momento da
  criação e mudá-lo exigia apagar e desenhar de novo.
- **`Alt` desenha retângulo e elipse a partir do centro**, combinável com
  `Shift`.
- **O histórico deixou de guardar cópias da imagem.** Cada edição virou uma
  operação registrada num log, e o estado visível é reconstruído a partir da
  imagem de partida. Um recorte custa um retângulo em vez de uma imagem
  inteira por passo. O log é limitado a 100 operações; ao estourar, a mais
  antiga é aplicada de vez à imagem de partida (descartá-la sem mais
  deixaria as anotações no espaço errado).
- **O rasterizador da exportação ganhou preenchimento**, cantos arredondados
  e traço contínuo por polilinha, que são a base das ferramentas acima. O
  traço contínuo acumula a cobertura de todos os segmentos antes de
  compor, para as junções não ficarem mais escuras que o resto — o que
  apareceria em traços translúcidos.

**Reconhecimento de texto**

- Lê o texto pelo motor do próprio Windows (`Windows.Media.Ocr`), o mesmo da
  Ferramenta de Captura — nada a instalar num Windows 11 limpo. Três portas
  de entrada: o atalho global `Ctrl+Alt+PrtScr`, o botão na barra do editor,
  e `rustshot --ocr <imagem>` para uso em linha de comando.
- A feature de compilação `ocr` passou a ser **padrão** agora que a
  funcionalidade tem interface. Continua existindo como feature para dar
  para medir o custo com `--no-default-features`, e porque é o único ponto
  do programa que depende da crate `windows` — o resto fala Win32 por
  `windows-sys`. Compilar sem ela continua válido: o atalho avisa em vez de
  fingir que funcionou.
- Quando o perfil do usuário não tem pacote de OCR, o motor cai no primeiro
  instalado em vez de falhar. Um Windows em pt-BR com apenas o pacote en-US
  — configuração comum — falhava antes com "The operation completed
  successfully", porque a API devolve nulo, e não erro, nesse caso.
- A imagem é ampliada 1,5× antes de reconhecer, truque emprestado do
  PowerOCR: o motor foi treinado para texto de documento e erra mais em
  fonte de interface no tamanho original.
- O custo no executável foi medido três vezes, em duas máquinas: **17.920
  bytes** nesta versão (release, LTO), e 14.848 e 16.384 bytes nas medições
  do protótipo. O exe tem 5,77 MB contra o alvo de 15 MB do CI. A crate
  `windows` não é dependência nova — já entrava pelo backend DX12 do wgpu
  desde a v1.3. Ver `docs/ocr-viabilidade.md`.

Mudanças internas que podem interessar a quem lê o código: as anotações
passaram de `Shape` para `Layer { id, shape, style }`, com identidade
estável; `editor/raster.rs` virou o módulo `editor/raster/`; e
`editor/ui.rs`, que tinha chegado a 1.620 linhas, virou o diretório
`editor/ui/` com uma responsabilidade por arquivo. O bloco de memória
compartilhada entre residente e GUI passou de `RSS1` para `RSS2`, porque
agora leva também a lista de janelas.

As conversões de pixel passaram de `chunks_exact(4)` para `as_chunks::<4>()`:
o clippy do Rust 1.98 passou a exigir a segunda forma quando o tamanho do
bloco é constante, e o CI quebrava por isso desde 22/08. O `ci.yml` continua
sem pin de toolchain, então um lint novo em qualquer stable futura pode
repetir o problema.

## v1.6.0 — 17/08

Versão dedicada a consumo de memória. O app ocupava ~90 MB parado na bandeja, e
praticamente nada disso eram dados do RustShot: era o device D3D12 que o
viewport-raiz de 1×1 px mantinha de pé a sessão inteira — driver da GPU mapeado,
blocos do alocador do wgpu e swapchain, 24 horas por dia, para uma janela
invisível de um pixel.

- **O processo residente não carrega mais GUI.** O executável passou a ter dois
  modos: o *residente* (sem argumentos) cuida de bandeja, atalhos globais e
  captura de tela cheia em Win32 puro, num loop de mensagens próprio, sem
  eframe/wgpu; e o de *GUI* (`--gui select|settings`), que sobe o eframe apenas
  para o overlay de seleção, o editor ou as configurações, e encerra ao terminar
  — devolvendo ao sistema tudo que a GPU custava. A captura continua acontecendo
  no residente, antes de qualquer janela existir, então a tela permanece
  congelada no instante do atalho; os pixels chegam ao processo de GUI por
  memória compartilhada. Balões de notificação e o aviso de "config.json
  regravado" voltam por `WM_COPYDATA`, porque atalhos e registro do Windows
  continuam sendo do residente (RF-05 segue valendo: alteração de atalho tem
  efeito imediato, sem reiniciar).
- **Ajustes de memória do backend gráfico** no processo de GUI: só
  `Backends::DX12` (o padrão do eframe fazia o wgpu sondar backends que nem
  estão compilados), `MemoryHints::MemoryUsage` — que troca os blocos de 256 MiB
  (device) e 64 MiB (host, RAM do sistema) do padrão por 8 MiB e 4 MiB —,
  `PowerPreference::LowPower`, que em máquina de GPU híbrida escolhe a integrada
  e evita carregar o driver da dedicada (`WGPU_POWER_PREF=high` reverte sem
  recompilar), e uma única frame em voo. Somou-se a isso o
  `trim_working_set()`, que devolve ao SO as páginas tocadas pela captura assim
  que o fluxo termina.
- **Executável 24% menor** (7,38 MB → 5,59 MB): saíram as quatro fontes
  embutidas do egui (Ubuntu-Light, Hack, NotoEmoji, emoji-icon-font), que o app
  não usava — a interface roda na Segoe UI Variable do sistema com a Inter
  embutida como reserva, e os ícones são vetoriais próprios —, saiu o
  `accesskit` e o release passou a usar `lto = "fat"`.
- **Alvo restrito a Windows 11 x64.** Build para outra arquitetura falha na
  compilação com mensagem explícita, e em sistema anterior à build 22000 o app
  mostra uma caixa de mensagem e encerra.

Mudanças de comportamento a conhecer:

- O overlay de seleção agora aparece algumas centenas de milissegundos depois do
  atalho — é o tempo de subir o processo de GUI e criar o device. A imagem
  exibida é a do instante do atalho, não a do momento em que a janela surge.
- **Leitores de tela não enxergam mais a interface**: o provedor de UI Automation
  (`accesskit`) foi removido em troca de memória.
- Emoji digitado no editor não renderiza (sem a NotoEmoji). Ele já não sobrevivia
  à exportação, que rasteriza com a Inter — o preview passou a ser fiel ao JPG.
- "Sair" com o editor aberto encerra apenas o residente: a janela do editor
  continua viva de propósito, para não descartar anotações não salvas.
- Falhas de registro de atalho aparecem como balão da bandeja, não mais na lista
  dentro da janela de configurações.

## v1.5.1 — 13/08

- **Executável assinado** (Authenticode, SHA-256 com carimbo de tempo): a
  assinatura acontece no CI, com a chave privada guardada apenas como segredo
  do repositório. O `SHA256SUMS.txt` passa a ser calculado depois da
  assinatura, e cada release leva junto a parte pública do certificado
  (`rustshot-codesign.cer`).
- O certificado é autoassinado: ele comprova a autoria e denuncia adulteração,
  mas **não** remove o aviso do SmartScreen — para isso seria preciso um
  certificado de uma autoridade certificadora. Detalhes na seção "Assinatura
  digital" do README.

## v1.5.0 — 12/08

- **Recorte no editor** (issue #5): nova ferramenta **Recortar** (`C`) —
  arraste a área a manter (o resto escurece, com as dimensões em px) e
  confirme com `Enter` ou com o botão ✓; `Esc` descarta a marcação. As
  anotações acompanham o conteúdo recortado, recortes sucessivos se
  compõem e `Ctrl+Z` devolve a imagem anterior: o histórico passou a
  versionar imagem e anotações juntas.
- **Toolbar redesenhada**: uma única faixa compacta, só com ícones
  vetoriais (nítidos em qualquer DPI, desenhados pelo próprio app — sem
  depender de fontes de emoji) e dicas no hover; a espessura ganhou uma
  amostra visual e os controles viraram campos arrastáveis, no lugar dos
  dois blocos de sliders e botões de texto.
- As anotações agora são recortadas pela borda da imagem também no editor,
  como já aconteciam no arquivo salvo.

## v1.4.0 — 07/08

- **Anotações reposicionáveis** (issue #2): nova ferramenta **Mover** na
  toolbar — clicar numa linha, seta, retângulo, elipse ou texto seleciona a
  anotação (moldura tracejada) e arrastar a reposiciona; a mesclagem com a
  imagem continua acontecendo só ao salvar/copiar. O desfazer/refazer cobre
  também os movimentos.
- **Atalhos de ferramenta e ajuste pela roda** (issue #1): `M`/`L`/`S`/`R`/
  `E`/`T` selecionam Mover/Linha/Seta/Retângulo/Elipse/Texto; `Ctrl+roda`
  aumenta/diminui a espessura do traço — ou o tamanho da fonte, com a
  ferramenta Texto ativa. O zoom, antes em `Ctrl+scroll`, passa para a roda
  pura (o pan segue no botão do meio).
- **Atalhos do editor configuráveis** (issue #4): a janela de Configurações
  permite trocar a letra de cada ferramenta (com aviso de conflito) e
  escolher o que o `Ctrl+roda` ajusta — traço/fonte (padrão, zoom na roda
  pura) ou zoom (ajuste de traço/fonte na roda pura).
- **Desenho começa exatamente no ponto do clique** (issue #3): o início do
  arrasto era detectado pelo egui só após ~6 pt de movimento, atrasando o
  preview e deslocando a forma; o rastreio agora usa a posição do press
  desde o primeiro frame.

## v1.3.1 — 03/08

- **O editor abre já com o foco do teclado**: depois de selecionar a região no
  modo "Capturar e editar", `Ctrl+C` e `Ctrl+S` funcionam de imediato, sem
  precisar clicar na janela antes. A janela nascia sem foco porque o Windows
  devolve o primeiro plano ao app anterior quando o overlay fecha, e o
  *foreground lock* recusa o pedido de foco de quem não está em primeiro
  plano; a correção anexa a fila de entrada da thread em primeiro plano
  (`AttachThreadInput`) durante os primeiros frames da janela.
- Distribuição por **GitHub Releases**: o `rustshot.exe` passa a ser publicado
  como asset a cada tag `v*`, compilado no CI com o toolchain MSVC. Link
  permanente para a versão mais recente:
  `.../releases/latest/download/rustshot.exe`.

## v1.3.0 — 03/08

**Código standalone.** Fora o núcleo de GUI (`eframe`/`egui` + `wgpu`), todas
as dependências foram substituídas por código próprio chamando Win32
diretamente via `windows-sys` — dependências diretas caíram de 21 para 6, e
tudo o que roda no binário é auditável no repositório (detalhes no README,
seção "Dependências").

- Nova camada `platform/`: bandeja + menu + notificações + atalhos globais em
  uma única janela com `WndProc` próprio; captura GDI; clipboard `CF_DIB`;
  registro Run; mutex de instância única; diálogo de pasta; data/hora; logger.
- Codificador JPEG incorporado e reduzido do image-rs (MIT/Apache-2.0, FDCT
  do Independent JPEG Group — licenças preservadas nos arquivos).
- Rasterizador de anotações próprio na exportação (superamostragem 4×4).
- JSON, buffer de imagem e tipo de erro próprios.
- Visível ao usuário: notificações agora são balões da bandeja com o nome e o
  ícone do RustShot (antes toasts WinRT rotulados "Windows PowerShell") e o
  seletor de pasta usa o diálogo clássico do shell. `config.json`, atalhos e
  fluxos permanecem idênticos.
- 49 testes de unidade (eram 20).

## v1.2.0 — 03/08

- **Seleção de região persistente**: soltar o arrasto não conclui mais a
  captura — a seleção fica na tela até `Ctrl+C` (copia para a área de
  transferência) ou `Ctrl+S` (salva como arquivo); novo arrasto refaz,
  `Esc`/botão direito cancela.
- **Editor**: `Ctrl+C` copia **e fecha** a janela (antes permanecia aberta);
  `Ctrl+S` continua salvando e fechando.
- Preparação para repositório público: binário compilado removido do
  repositório (a distribuição é o artefato do CI), identificadores de máquina
  retirados da documentação e histórico do git reescrito sem dados pessoais.

## v1.1.0 — 02–03/08

- Renderizador **wgpu/Direct3D 12** (o backend OpenGL sofria *unredirection*
  pelo driver NVIDIA, apagando o monitor por ~1 s ao abrir a seleção).
- Visual **Windows 11 (Fluent)**: claro/escuro do sistema, cor de destaque,
  cards, Segoe UI Variable na UI.
- Saída fixa em **JPG** (qualidade 90) e todo o estado (`config.json` +
  `rustshot.log`) ao lado do exe (portátil por definição).
- Captura de região passou a copiar também para a área de transferência
  (comportamento revisto na v1.2).
- Correções: janela-raiz sem retângulo preto/Alt-Tab/Alt+F4, pré-carga de
  texturas do overlay, origem do arrasto via `press_origin`, reserva atômica
  de nomes de arquivo, filtro de módulos gráficos no log.
- Relatório de testes e auditoria de segurança em
  `docs/relatorio-de-testes-v1.1.md`.

## v1.0.0 — 02/08

Implementação inicial da Especificação Técnica v1.0: três modos de captura
por atalhos globais (tela cheia, região, região + edição), editor com 5
ferramentas e undo/redo, multi-monitor com DPI Per-Monitor V2, bandeja do
sistema, configuração persistente com efeito imediato, instância única,
"Iniciar com o Windows" e exe único sem instalador.
