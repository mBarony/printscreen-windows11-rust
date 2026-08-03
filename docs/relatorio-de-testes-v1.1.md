# Relatório de Testes — RustShot v1.1

**Data:** 02–03/08/2026 · **Máquina:** Windows 11 Pro (10.0.26200), GPU NVIDIA, monitor Samsung Q85A 4K (3840×2160) com escala 150% · **Toolchain:** Rust 1.97.1 stable MSVC · **Smart App Control:** ativo (modo imposição)

Método geral dos testes de interface: injeção de entrada sintética via Win32
(`keybd_event` para atalhos globais, `SetCursorPos`+`mouse_event` para
arrastos, processo de teste DPI-aware operando em pixels físicos), com
verificação por log do app, inspeção de janelas (EnumWindows/GetWindowRect/
GetWindowLongPtr), leitura de pixels (CopyFromScreen/GetPixel) e análise dos
arquivos gerados.

## 1. Testes de unidade

`cargo test` — **20/20 aprovados**: parsing/roundtrip de cores, tolerância a
config parcial e a config legado com `image_format` (v1.0), geometria de
formas (Shift→45°/quadrado, seta mínima), undo/redo, parsing de atalhos,
conflitos de atalho, sanitização de nomes de arquivo Windows, sufixos de
colisão `_1`/`_2`, template com extensão digitada (sem `.jpg.jpg`), reserva
atômica de caminho, renderização (dimensões preservadas, linha/texto tocam
pixels, render vazio = identidade).

## 2. Funcional de ponta a ponta (build release, nesta máquina)

| Teste | Método | Resultado |
|---|---|---|
| Primeira execução | apagar estado, lançar, inspecionar pasta do exe | `config.json` com padrões + `rustshot.log` **ao lado do exe**; 3 atalhos registrados; `%APPDATA%` não utilizado ✅ |
| Tela cheia (`Ctrl+PrtScr`) | hotkey sintético; validar arquivo | JPG válido (header FFD8FF) **3840×2160 px físicos** (DPI Per-Monitor V2 correto) ✅ |
| Região (`Shift+PrtScr`) | arrasto físico 800×500 em (600,400)→(1400,900) | recorte **exatamente 800×500**; salvo em `Imagens\RustShot` ✅ |
| Região → área de transferência | comparar clipboard após captura | imagem **800×500 idêntica** ao arquivo ✅ |
| Estresse de seleção (3×) | cursor "teleportado" + clique quase imediato (pior caso de pointer obsoleto) | 800×500, 600×400, 500×500 — **todos exatos** ✅ |
| Editor (`Ctrl+Shift+PrtScr`) | arrasto + seta no canvas + `Ctrl+S` | "captura anotada salva"; seta presente no JPG (pixels #FF3B30 verificados); editor fecha ✅ |
| Estado ocupado | hotkey durante edição | captura ignorada com toast, sem novo fluxo ✅ |
| Instância única | lançar 2º processo | detecta mutex, notifica e encerra; 1 processo restante ✅ |
| Autostart | ler `HKCU\...\Run` com `start_with_windows=false` | entrada ausente ✅ |
| Colisão de nomes | duas capturas no mesmo segundo (reserva atômica) | arquivos distintos `nome.jpg`/`nome_1.jpg` (teste de unidade + código com `create_new`) ✅ |

## 3. Janela-raiz (correção do retângulo preto da v1.0)

| Verificação | Método | Resultado |
|---|---|---|
| Invisível ao usuário | GetWindowRect/IsWindowVisible | visível ao SO (necessário para WM_PAINT/atalhos), mas 2×2 px físicos em (-32768,-32768) — fora de qualquer tela ✅ |
| Não rouba foco no launch | GetForegroundWindow antes/depois | foreground inalterado (`with_active(false)`) ✅ |
| Fora do Alt-Tab | GetWindowLongPtr EXSTYLE | `WS_EX_TOOLWINDOW` presente, `WS_EX_APPWINDOW` ausente (reaplicado a cada update — winit reescreve o estilo) ✅ |
| Imune a Alt+F4 | foco na janela + Alt+F4 sintético | app sobrevive; atalhos seguem funcionando (CancelClose) ✅ |
| Win+Seta não maximiza | builder sem resize/maximize | bloqueado por configuração da janela ✅ |

## 4. Visual Windows 11 (Fluent)

Método: janela de Configurações aberta via hook de debug (`RUSTSHOT_OPEN=settings`),
editor via fluxo real; screenshots das janelas inspecionados visualmente.
Resultado: tema escuro seguindo o sistema, cor de destaque do Windows nos
controles, cards arredondados, Segoe UI Variable na UI, toggle estilo Win11,
toolbar do editor em duas linhas sem cortes ✅. Texto de anotação permanece na
Inter embutida (família nomeada), preservando WYSIWYG com a exportação ✅.

## 5. Apagão do monitor na seleção de região (relato do usuário)

Método de diagnóstico: triangulação com o usuário — o apagão era visível a
olho nu mas ausente em gravação (Snipping Tool), apontando fenômeno de
scanout, não de conteúdo; `Win+Shift+S` **não** piscava (descartou driver/
display globalmente); G-Sync já estava "full screen only" e desativar
otimizações de tela cheia não mudou (descartaram-se as causas clássicas).
Conclusão: *unredirection* de janela **OpenGL** de tela cheia pelo driver
NVIDIA (o backend glow do eframe). Correção: migração do renderizador para
**wgpu/Direct3D 12** (mesmo caminho DXGI/DWM composto do Snipping Tool).
Testes pós-migração nesta máquina: todos os fluxos funcionais re-executados
e exatos (seções 2–3). **Confirmação visual do fim do apagão: depende do
usuário** (impossível observar o scanout remotamente).

Correções complementares testadas: pré-carga das texturas do overlay antes
da criação das janelas (elimina o 1º frame preto da própria janela) e origem
do arrasto via `press_origin()` (elimina recorte errado intermitente com
pointer obsoleto no frame de nascimento da janela — reproduzido e corrigido).

## 6. Auditoria de segurança

### 6.1 Exfiltração — nada sai da máquina

| Verificação | Método | Resultado |
|---|---|---|
| Código de rede | grep no fonte (sockets/HTTP/spawn) | zero APIs de rede; único processo externo: `explorer <pasta>` (menu) ✅ |
| Dependências de rede | varredura do `Cargo.lock` (reqwest/hyper/tokio/rustls/socket2/mio/curl/tls…) | **nenhum crate com capacidade de rede** no binário ✅ |
| Conexões em runtime | `Get-NetTCPConnection`/`Get-NetUDPEndpoint` por PID em 5 momentos (launch, tela cheia, overlay, região+clipboard, idle) | **0 TCP / 0 UDP em todos** ✅ |
| Escritas em disco | snapshot de pastas antes/depois | apenas estado ao lado do exe + JPGs no destino; sem temporários ✅ |
| Registro | revisão de código | leitura: cor de destaque (DWM); escrita: só `HKCU\...\Run` sob comando do usuário ✅ |

### 6.2 Escopo de captura — só o que foi selecionado

| Verificação | Método | Resultado |
|---|---|---|
| Exatidão do recorte | janela-padrão com 4 quadrantes coloridos em posição conhecida; captura da região; leitura dos quadrantes no JPG | desvio **0–1 (de 255)** nos 4 quadrantes — posição, dimensão e conteúdo exatos ✅ |
| Metadados no arquivo | enumeração dos segmentos JPEG | apenas JFIF/SOF/DQT/DHT — **sem EXIF, XMP ou comentários** ✅ |
| Clipboard | monitoração durante fluxos | escrito apenas em ações explícitas (região; Ctrl+C no editor) ✅ |
| Capturas transitórias | revisão de código + snapshot de arquivos | congelamento multi-monitor vive só em memória durante a seleção; nada persistido além do recorte ✅ |

### 6.3 Achado corrigido

O naga (shaders do wgpu) despejava código-fonte de shader no `rustshot.log`
em nível Info. Módulos gráficos (`naga`, `wgpu*`, `egui_wgpu`) filtrados do
logger; log re-verificado limpo ✅.

## 7. Revisão adversarial multi-agente (v1.1)

29 agentes em 2 fases (4 dimensões de revisão + verificação cética de cada
achado): 25 achados brutos → **24 confirmados → todos corrigidos** e
re-testados (destaques: pasta não-gravável com aviso, `load()` sem clobber do
config em erro transitório, reserva atômica de nomes, janela-fantasma fora do
Alt-Tab/imune a Alt+F4, VERSIONINFO 1.1.0, correções de documentação).

## 8. Builds sob Smart App Control

O SAC (vereditos de reputação por hash, permanentes) bloqueava artefatos
intermediários de compilação de forma intermitente (`os error 4551`).
Solução institucionalizada e testada: `-Cmetadata` fixo em
`.cargo/config.toml` + `[profile.release.build-override]` com flags dos
build scripts idênticas às do dev — o último build release compilou
**sem nenhum bloqueio**. `rustshot.exe` final: 9,82 MB (RNF-01 ≤ 15 MB ✅),
VersionInfo 1.1.0 ✅.
