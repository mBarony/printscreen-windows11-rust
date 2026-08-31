# Revisão de código — v1.11.0

Revisão das 27.055 linhas em 83 arquivos, feita em 31/08/2026 sobre o commit `fd94912`.

## Método

O código foi dividido em seis fatias revisadas em paralelo, cada uma com o critério do projeto como régua (simplicidade, complexidade ciclomática ≤ 10, uma responsabilidade por módulo, comentário que explica o porquê): `qr/`, `editor/` (núcleo), `editor/ui/` + `editor/raster/`, `platform/`, o fluxo (`app.rs`, `main.rs`, `resident.rs`, `jobs.rs`, `overlay.rs`), e o resto de `src/*.rs` mais `jpeg/`.

Todo achado que alegava **bug** passou por um segundo revisor cujo trabalho era derrubá-lo: conferir se o trecho existe como descrito, se o cenário é alcançável a partir de como o programa é usado, se não há guarda antes que já o impeça, e se a correção proposta não quebra outra coisa. Dois achados caíram nesse filtro e estão registrados abaixo — o valor de anotá-los é não voltarem a ser levantados.

Duas verificações não chegaram a rodar (limite de sessão): `config.rs parse_color` e `imgout.rs` heurística de formato. Os dois estão marcados como **não verificados**.

O dead code foi levantado mecanicamente, tirando as supressões `#[allow(dead_code)]` e deixando o compilador falar, em vez de procurar a olho.

## O que foi corrigido nesta passagem

### QR: o decodificador não lia nada acima da versão 7

Três defeitos somados, todos em `src/qr/detecta.rs`, todos no código entregue na v1.11.0 no mesmo dia.

**A proporção 1:1:3:1:1 era conferida antes de a última faixa fechar.** O teste rodava a cada pixel escuro *dentro* da quinta faixa, não quando ela terminava. Uma faixa truncada passa na tolerância de meio módulo, vira um candidato com o módulo subestimado, e `agrupa` tira a média dele com o candidato bom — o módulo saía 2,4% menor. Esse erro é multiplicado pela distância entre os localizadores, então quanto maior o símbolo, pior: medido, de v13 em diante o símbolo era amostrado com o lado de uma versão a mais, e a v40 era recusada. A verificação independente foi além do achado original e mediu que, na maioria dos tamanhos de módulo, **qualquer QR acima da versão 7 falhava** — ou seja, a partir de ~110 bytes. Um PIX copia-e-cola, um vCard ou uma URL longa nunca decodificariam.

**A amostragem mirava a borda do módulo, não o meio.** O deslocamento era `mx − 3,5`, em coordenada de borda, quando `tl.x` é o centro do localizador e portanto está em centro de módulo: o certo é `mx − 3`. Com 3,5 a amostra caía no primeiro pixel do módulo. Num símbolo desenhado 1:1 isso funciona por acidente — o primeiro pixel tem a cor do módulo —, mas basta o símbolo ser reamostrado (zoom de página, escala de DPI, imagem redimensionada) para a borda carregar a cor do vizinho. Medido depois da correção: de 7 reduções fracionárias, o código antigo lia 2 e o novo lê 7.

**O voto de vizinhança era um voto de um só.** `voto` amostrava em deslocamentos de ±0,25 **pixel** quando a intenção — escrita no próprio comentário — era um quarto de **módulo**. Com módulo de 8 px os cinco votos caíam no mesmo pixel. Agora o passo é fração do módulo, medida no espaço da imagem.

Junto veio o meio pixel de viés em `centro_x`, que era calculado em coordenada de borda enquanto `centro_y` já vinha em índice de pixel.

**Teste que faltava:** o caminho da imagem só era exercitado até a versão 5; as versões altas eram testadas a partir da `Grade` pronta, que pula o detector inteiro. Agora `le_todas_as_versoes_pela_imagem` cobre v1 a v40 em duas escalas, e `le_com_modulo_de_tamanho_quebrado` cobre módulo fracionário. Os dois falham no código antigo.

### QR: designador ECI de 2 e 3 bytes

`src/qr/dados.rs`. O prefixo do designador era testado por `>> 5`, o que jogava o caso de 2 bytes (prefixo `10`) no ramo "nenhum byte extra". Ler menos do que o designador ocupa não perde só o designador: desalinha todo o resto do fluxo, e o segmento seguinte vira lixo ou faz o símbolo ser recusado. Agora a decisão é pelos bits altos, com prefixo inválido recusado.

### Dead code removido

**`SAC_EXE_SALT`** (`src/main.rs`) — constante que prometia re-rolar o hash do exe quando o Smart App Control bloqueasse o binário. Não era referenciada por nada, e uma `const` não usada nem chega ao binário: trocá-la nunca mudaria hash nenhum. O mecanismo que funciona de verdade é o rótulo `-Cmetadata` em `.cargo/config.toml`, re-rolado por `build.ps1 -NewSalt`, e é o que o README documenta.

**`#![allow(dead_code)]` de `platform/ocr.rs`** — a supressão era do tempo em que o módulo era prova de conceito, e o comentário dizia "o allow sai junto com o botão". O botão existe desde a v1.9. Removida a supressão, o compilador acusou o que ela escondia: `available_languages` e `recognize_words` reexportados sem ninguém consumir. `recognize_words` voltou a ser interno (é o intermediário entre `recognize` e `recognize_boxes`), `available_languages` virou `#[cfg(test)]` — só o teste de motor real a usa, para dizer "instale um pacote de idioma" em vez de falhar com "não reconheceu nada" — e o stub de não-Windows dela saiu.

**Bloco de criação de guias duplicado** (`src/editor/ui/interact.rs`) — `Alt+H`, `Alt+V` e `Alt+Shift+G` eram tratados em dois lugares com código idêntico. `handle_layer_keys` roda antes e consome as teclas, então a cópia dentro de `restyle_selection` nunca executava no caminho normal. Pior: quando `handle_layer_keys` desiste cedo (campo de texto em foco, confirmação de descarte), a cópia rodava e criava uma guia com a tecla que o campo de texto deveria receber. O tratamento ficou só no módulo de teclas.

**`cursor_pos` duplicada** — existia com corpo idêntico em `platform/scroll.rs` e `platform/capture.rs`. Ficou a de `capture`, que já era reexportada; `scroll` volta a ter uma responsabilidade só, que é o que o cabeçalho do módulo promete.

## Correções confirmadas — todas aplicadas em 31/08/2026 (v1.11.2)

Todas passaram pelo revisor adversarial, e todas foram corrigidas. O que está
escrito abaixo é o diagnóstico como ele foi levantado; ficou de propósito, para
o dia em que alguém quiser saber por que o código é como é.

Ao corrigir, apareceram **três defeitos que a revisão não tinha visto**, cada um
encostado num dos que ela viu:

- as anotações consolidadas em `baseline.layers` quando o log estoura o teto
  **não eram gravadas em lugar nenhum**, então a sessão recuperada voltava sem
  as anotações mais antigas mesmo com a imagem certa;
- o campo de porcentagem não só gravava uma operação por quadro: como ele
  renascia em 100% a cada quadro, cada operação escalava de novo em cima da
  anterior — arrastar até 50% **não dava metade**;
- a seleção fantasma não era só do `perform_redo`. Outros três pontos limpavam
  `selected` sem limpar `selection` (aplicar recorte, cortar faixa e o primeiro
  `Esc`), e a correção foi tirar do par a possibilidade de divergir.

Estão em ordem de gravidade.

### A recuperação de sessão para de gravar depois de 100 edições

`src/editor/document.rs:437`. `commit` faz `self.index = self.ops.len()` **antes** do laço que aplica o teto de `MAX_OPS = 100`, e esse laço faz `remove(0)` e `self.index -= 1`. Da 101ª operação em diante `applied()` fica preso em 100 — e `persist_session` usa esse número como assinatura de sujeira para decidir se regrava o `session.json`. Resultado: numa sessão longa, tudo a partir da 101ª edição nunca chega ao disco. Se o processo morrer na edição 140, a recuperação devolve o documento como estava na edição 100.

Correção proposta: um contador monotônico de edições no `Document` (`u64` incrementado em `commit`, `undo`, `redo` e `reset_crop`), usado por `persist_session` no lugar de `applied()`.

### A sessão recuperada usa a imagem de origem errada

`src/editor/session_file.rs:42`. A imagem de origem é gravada uma vez só, protegida por `source_saved`, que nunca é revertido. Mas quando o log estoura o teto, `Document::commit` **muta** `baseline.image` aplicando a operação mais antiga. A partir daí o `session.json` referencia uma imagem de origem que não corresponde mais à base do documento, e a recuperação reconstrói outra coisa. Anda junto do achado anterior e provavelmente deve ser corrigido no mesmo movimento.

### A captura com atraso dispara em hora imprevisível, ou nunca

`src/resident.rs:219`. `capture_after_delay` dorme 3 s numa thread e empilha um evento num `Mutex<Vec<_>>`, mas não posta mensagem nenhuma para a janela. A fila só é drenada depois de o laço de mensagens despachar algo — então o disparo acontece no próximo evento que chegar por acaso (um clique na bandeja, um atalho), e numa máquina parada pode não acontecer. Correção: acordar o laço, como os outros caminhos já fazem.

### `launch_recover` engole o atalho seguinte

`src/resident.rs:589`. Todos os pontos que lançam um processo filho armam o `set_poll_timer(true)`, que é o que recolhe o filho quando ele termina — menos este. O editor recuperado nunca é dado como encerrado, e o próximo atalho é ignorado porque o residente ainda se considera ocupado.

### O remendo encostado na borda copia o próprio objeto

`src/editor/heal.rs:45`. O comentário promete que "onde o buraco encosta na moldura da imagem não há vizinho: vale a aresta oposta". O código usa `saturating_sub(1)`, que na linha 0 devolve a própria linha 0 — dentro do buraco. A condição de contorno da equação de Laplace passa a ser a cor do objeto que se quer apagar, e o remendo reconstrói o objeto em vez do fundo.

### A corrida de edição coalescida nunca coalesce

`src/editor/ui/canvas.rs:192`. A guarda de arrasto órfão (`if !primary_down && !primary_released { session.drag = None; cancel_move(session); }`) roda em quase todo quadro, porque o ponteiro fica solto na maior parte do tempo. `cancel_move` fecha a corrida de edição, então o coalescimento de empurrões por seta e de ajustes pela roda — que existe justamente para não gerar um passo de desfazer por quadro — não acontece nunca.

### O campo de porcentagem grava uma operação por quadro

`src/editor/ui/toolbar.rs:579`. `percent` é recriado a cada quadro e `response.changed()` dispara `doc.scale()` durante o arrasto do controle. Cada chamada é um `commit` que dispara um `replay` completo, e cada replay reaplica todas as escalas anteriores. Arrastar o controle de 100% a 50% deixa dezenas de operações de escala no histórico e faz o custo do replay crescer com o número delas.

### `perform_redo` deixa seleção fantasma

`src/editor/ui/mod.rs:247`. Limpa `selected` mas não `selection`. A tela mostra anotações marcadas como selecionadas que não respondem a nada.

### O caminho de "ocultar só as palavras" sai da closure do canvas

`src/editor/ui/canvas.rs:440`. Um `return` de dentro da closure do `CentralPanel` pula o bloco de desenho, e o canvas fica sem pintura por um quadro.

### Não verificados

- `src/config.rs:642` — `parse_color` fatia a string por índice de byte (`&h[0..2]`) depois de testar `h.len()`, que também é em bytes. Um caractere multibyte no valor de cor do `config.json` faz o fatiamento cair no meio de um code point e o processo entra em pânico. Plausível e barato de confirmar; a leitura do config é justamente o caminho que deveria ser tolerante a arquivo estragado.
- `src/imgout.rs:88` — a amostragem da heurística que escolhe entre PNG e JPG colapsa em poucas colunas em resoluções comuns, ao contrário do que o comentário promete.

## Achados derrubados

Registrados para não voltarem.

**`GetCanonicalFormatEtc` devolvendo `S_FALSE` com o `FORMATETC` de saída não inicializado** (`platform/dragdrop.rs:211`). O objeto só é entregue ao `DoDragDrop`; o app não registra `IDropTarget` e não há chamador in-process. O revisor não encontrou caminho pelo qual um alvo de drop real chegue a ler os campos não inicializados. Continua sendo um contrato COM cumprido pela metade — vale arrumar quando o arquivo for mexido —, mas não é bug alcançável.

**Vazamento de HGLOBAL quando `GlobalLock` falha** (`platform/dragdrop.rs:171`). O cenário alegado era memória esgotada durante o arrasto, e `GlobalLock` não aloca: com `GMEM_MOVEABLE` sem `GMEM_DISCARDABLE` o bloco já é comprometido no `GlobalAlloc`, e o Lock só incrementa uma contagem. O ramo de falha é inalcançável na prática.

## Otimizações

Em ordem de ganho medido.

**`blend` chama `f32::round()` quatro vezes por pixel** (`src/editor/raster/mod.rs:38`). No alvo x86-64 padrão cada `round()` é uma chamada de biblioteca. Medido com o código real extraído para um binário otimizado, reproduzindo `paint_shadow` sobre um canvas de 2048×1208: 8,07 s contra 1,84 s trocando `.round() as u8` por `+ 0.5) as u8` — **4,4×**, com resultado de pixel idêntico, porque os operandos são sempre combinação convexa de bytes.

**`fill_rect` superamostra 16 vezes o miolo do retângulo** (`src/editor/raster/fill.rs:256`), onde a cobertura é sempre 1. No mesmo cenário: 829 ms contra 94 ms com um atalho para os pixels inteiramente contidos na cruz interna do retângulo arredondado.

**O replay refaz tudo a cada edição** (`src/editor/document.rs:392`). Desenhar uma seta numa captura 1080p com moldura decorativa refaz redação, holofote, remendo e a moldura — da ordem de 1,3 bilhão de avaliações de `inside_round_rect`. As comparações que diriam "nada disso mudou" já existem no arquivo; falta antecipá-las e reaproveitar os `Arc` guardados.

**`agrupa` é quadrático** (`src/qr/detecta.rs:238`). Cada candidato é comparado com todos os grupos já abertos. Medido com uma textura de quadradinhos: 600×600 → 43 ms; 1200×1200 → 650 ms; 2400×2400 → **10,6 s**, dos quais 10,58 s dentro de `agrupa`. Como `decode` roda em toda seleção do comando de reconhecer, uma captura grande de uma folha de etiquetas ou de uma textura regular trava o editor por segundos. É o achado mais urgente desta seção.

**`publish` copia a captura duas vezes** (`src/platform/ipc.rs:266`): para um `Vec` intermediário e depois para a view mapeada. Em dois monitores 4K são ~66 MB de alocação transitória mais um memcpy de 66 MB, exatamente na latência entre apertar o atalho e o overlay aparecer.

**Cópia integral da tela cheia no residente** quando o destino é "salvar e copiar" (`src/resident.rs:354`).

## Simplificações

**`canvas::draw` tem 572 linhas** e complexidade ciclomática na casa das centenas (`src/editor/ui/canvas.rs:32`). Dois dos bugs desta revisão — a guarda de arrasto órfão e o `return` que pula o desenho — existem porque estão a centenas de linhas do código que depende deles. A extração proposta é mecânica: uma função por braço do match de interação, mais uma para o bloco de desenho.

**`process_shared` tem ~220 linhas** e seis transições (`src/app.rs:134`). A condição de `quit` depende de cinco campos escritos em cinco blocos diferentes acima dela.

**`run_gui` e `run_event_loop` montam `NativeOptions` idênticos** (`src/main.rs:282`), ~20 linhas duplicadas.

**Dois blocos de doc comment presos ao campo errado** do `EditorSession` (`src/editor/mod.rs:201`).

## Complexidade e tamanho, medidos

`clippy::cognitive_complexity` acima do limite padrão (25): `app.rs:134 process_shared` (28), a closure de `overlay.rs:330` (38) e a closure de `canvas.rs:35` (**83**).

Vinte módulos passam do gatilho de 400 linhas do projeto. Os maiores: `editor/shapes.rs` (1707), `editor/document.rs` (1355), `main.rs` (921), `editor/ui/toolbar.rs` (847), `app.rs` (824), `config.rs` (783), `overlay.rs` (753).

Nenhum dos dois é defeito por si — são gatilhos de revisão, e estão registrados aqui como tal.

## Estado depois desta passagem

**Ao fim da revisão** (v1.11.1): 381 testes passando, `clippy --all-targets` sem avisos, nas duas configurações de feature. As supressões de `dead_code` que restam são todas de stub `#[cfg(not(windows))]`, que existem para o porte e não escondem nada.

**Depois de aplicar as nove correções** (v1.11.2): 389 testes, com 8 de regressão novos. Cada um foi provado falhando no código antigo antes de entrar — é o que separa um teste que cobre de um teste que acompanha.

Dois deles mereciam nota por serem de interface, que costuma ser a desculpa para não testar: o canvas e a barra do editor são exercitados por um `egui::Context` sem GPU e sem janela, com `ctx.run` e eventos de ponteiro sintéticos. Foi assim que a corrida de edição coalescida e o arrasto do campo de porcentagem viraram testes de verdade, em vez de raciocínio escrito no relatório.

O que continua em aberto desta revisão: as seis otimizações, as quatro simplificações e os dois achados não verificados.
