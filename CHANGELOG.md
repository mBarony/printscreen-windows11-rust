# Changelog

Histórico de versões do RustShot. Datas em 2026.

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
