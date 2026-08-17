# Changelog

Histórico de versões do RustShot. Datas em 2026.

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
