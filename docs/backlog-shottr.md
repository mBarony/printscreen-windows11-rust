# Backlog — paridade com o Shottr

Levantamento de tudo que a [página do Shottr](https://shottr.cc) anuncia,
confrontado com o que o RustShot v1.8.0 já faz.

O Shottr é um app de captura para macOS. Não é um concorrente direto: ele
existe num sistema com APIs que aqui não existem, e algumas funcionalidades
dele são baratas lá e caras aqui — ou o contrário. O confronto serve para
escolher o que vale trazer, não para copiar a lista inteira.

**Legenda:** ✅ existe · 🔶 existe em parte · ⬜ falta
**Esforço:** P = até um dia · M = alguns dias · G = uma semana ou mais

---

## 1. Captura

| | Funcionalidade | Estado | Esforço |
|---|---|---|---|
| ✅ | Região por arrasto | copia direto desde a v1.8.0 | — |
| ✅ | Tela cheia | com escopo configurável (todos / principal / sob o cursor) | — |
| ✅ | Janela inteira | `Space` no overlay, com bordas reais pelo DWM | — |
| ⬜ | **Captura com rolagem** | costura uma página longa em várias passadas | **G** |
| ⬜ | Repetir a última região | recaptura o mesmo retângulo sem arrastar de novo | P |
| ⬜ | Captura com atraso | conta 3 s antes de congelar a tela | P |
| ⬜ | Seleção inteligente | detecta o elemento sob o cursor e ajusta a seleção | G |
| ⬜ | Seleção quadrada | `Shift` durante o arrasto trava a proporção | P |

A **captura com rolagem** é a funcionalidade mais citada do Shottr e a que mais
falta aqui. No Windows não há API que role uma janela alheia de forma
confiável: o caminho é enviar `WM_MOUSEWHEEL`, capturar, e costurar por
correlação de faixas sobrepostas — que é onde mora o trabalho. Vale como item
único e grande, não fatiado.

## 2. Anotação

| | Funcionalidade | Estado | Esforço |
|---|---|---|---|
| ✅ | Texto | multilinha, com pílula de fundo opcional | — |
| ✅ | Mão livre | suavizado por Béziers | — |
| ✅ | Seta | reta | — |
| ✅ | Retângulo e elipse | com preenchimento e cantos arredondados | — |
| ✅ | Marca-texto | traço grosso translúcido | — |
| ✅ | Contador numerado | renumera sozinho ao apagar | — |
| ✅ | Holofote | três formas, com ampliação — equivale à lupa do Shottr | — |
| ✅ | Ocultar | mosaico sintético ou cor chapada | — |
| ✅ | Desfazer/refazer | op-log com replay | — |
| ✅ | Duplicar | `Alt+D` | — |
| 🔶 | Estilos de linha | só sólida; falta tracejada e pontilhada | M |
| ⬜ | **Seta em arco** | alça central que dobra a seta | M |
| ⬜ | Seta reversível | inverte a ponta sem redesenhar | P |
| ⬜ | **Estilo desenhado à mão** | variante rabiscada para seta, formas e texto | M |
| ⬜ | Ocultar só o texto | usa o OCR já presente para achar e borrar apenas as letras | M |
| ⬜ | Remover objeto | apaga um elemento e preenche o buraco | G |
| ⬜ | Colar imagem sobre a captura | sobrepõe outra imagem como camada | M |
| ⬜ | Duplicar arrastando | `Alt`+arrasto em vez de `Alt+D` | P |
| ⬜ | Espaço move o objeto em desenho | reposiciona sem soltar o arrasto | P |
| ⬜ | Copiar e colar anotações | entre capturas | M |

## 3. Medição e cor

| | Funcionalidade | Estado | Esforço |
|---|---|---|---|
| ✅ | Conta-gotas | amostra a cor e volta à ferramenta anterior | — |
| ✅ | Paleta configurável | 8 cores + seletor livre | — |
| ⬜ | **Régua de tela** | mede distâncias em px, com setas nas pontas | M |
| ⬜ | Cor média de uma área | em vez de um pixel só | P |
| ⬜ | Cor do texto sob o cursor | o tom mais escuro num quadrado de 20×20 | P |
| ⬜ | Formatos de cor | OKLCH e contraste APCA além do HEX | P |
| ⬜ | Guias | linhas de apoio horizontais e verticais | P |

## 4. Imagem

| | Funcionalidade | Estado | Esforço |
|---|---|---|---|
| ✅ | Recortar | com confirmação por `Enter` | — |
| ✅ | Remover faixa | joga fora uma tira e junta o resto | — |
| ✅ | Moldura decorativa | quatro gradientes com sombra e cantos | — |
| ✅ | Zoom e pan | roda e botão do meio | — |
| ⬜ | Redimensionar a captura | escalar a imagem inteira | P |
| ⬜ | Desfazer o recorte | voltar ao enquadramento original | P |
| ⬜ | Semitransparência | deixar a imagem translúcida | P |
| ⬜ | GIF antes/depois | dois quadros comparativos | M |

## 5. Reconhecimento de texto

| | Funcionalidade | Estado | Esforço |
|---|---|---|---|
| ✅ | OCR da região | atalho global, copia direto | — |
| ✅ | OCR da imagem aberta | botão na barra do editor | — |
| ✅ | Vários idiomas | os pacotes instalados no Windows | — |
| ⬜ | **Leitura de QR code** | decodifica o código na seleção | M |
| ⬜ | Reconstruir colunas | preserva tabelas usando a geometria das palavras | G |

O QR é o item de melhor relação valor/esforço aqui: o Windows não tem
decodificador embutido, mas o algoritmo é bem delimitado e não exige
dependência pesada.

## 6. Fixar na tela

| | Funcionalidade | Estado | Esforço |
|---|---|---|---|
| ⬜ | **Fixar a captura como janela flutuante** | sempre no topo, sem bordas | M |
| ⬜ | Redimensionar pela roda | ajusta a janela fixada | P |

Ficou de fora do port do omasnap por decisão explícita. Continua sendo o item
mais pedido de quem usa esse tipo de app, e no Windows é uma janela
`WS_EX_TOPMOST` sem bordas — trabalho moderado e sem armadilha conhecida.

## 7. Saída e integração

| | Funcionalidade | Estado | Esforço |
|---|---|---|---|
| ✅ | Copiar e salvar | com pasta configurável | — |
| ✅ | Atalhos configuráveis | quatro globais, mais os do editor | — |
| ✅ | Bandeja do sistema | menu com todos os modos | — |
| ✅ | Notificações | balões da bandeja | — |
| 🔶 | Formato de saída | só JPG q90; falta PNG e escolha automática | M |
| ⬜ | Arrastar para outro app | tirar a captura do editor arrastando | M |
| ⬜ | Upload para S3 | e qualquer armazenamento compatível | G |
| ⬜ | Salvar e copiar automáticos | sem passar pelo editor | P |
| ⬜ | API por URL | automação externa | M |

**PNG merece uma nota.** A saída em JPG foi uma decisão consciente (sem
codificador extra no binário), mas ela machuca justamente o caso mais comum
de captura de tela: texto e interface, onde o JPG borra as bordas. O Shottr
escolhe o formato pelo conteúdo. Trazer isso exige um codificador PNG — algo
entre reaproveitar o `deflate` que já entra pelo `png` do wgpu e escrever um
compressor simples.

---

## 8. Levar para Linux e macOS

Esta parte foi pedida para depois de o Windows estar completo, e é bom que
seja: ela **não é um port, são dois aplicativos novos**. Vale ver o tamanho
com clareza antes de começar.

O RustShot é declaradamente Windows 11 x64 — há um `compile_error!` para
outros alvos, e o commit que fixou isso é intencional. Tudo que toca o sistema
está em `src/platform/`, e é Win32 puro:

| Módulo | O que usa hoje | Linux | macOS |
|---|---|---|---|
| `capture` | BitBlt + GDI | X11 razoável; **Wayland exige portal** | ScreenCaptureKit |
| `clipboard` | `CF_DIB` | X11/Wayland diferentes entre si | NSPasteboard |
| `shell` (bandeja, atalhos) | Shell_NotifyIcon, RegisterHotKey | StatusNotifierItem, varia por desktop | NSStatusItem |
| `window_list` | EnumWindows + DWM | X11 sim; **Wayland não expõe** | CGWindowList |
| `imagefile` | GDI+ | decodificador próprio | ImageIO |
| `ocr` | `Windows.Media.Ocr` | **não há motor de sistema** | Vision |
| `autostart` | registro | `.desktop` | LaunchAgent |
| `instance`, `ipc` | mutex, `WM_COPYDATA` | socket | XPC ou socket |
| `msgbox`, `version`, `memory` | Win32 | — | — |

Três obstáculos que não são só trabalho:

1. **Wayland.** Captura de tela passa pelo `xdg-desktop-portal`, que em várias
   configurações mostra um diálogo de permissão **a cada captura** — o que
   quebra a proposta de "atalho e pronto". Atalhos globais são um problema
   conhecido e sem solução uniforme. Enumerar janelas alheias não é permitido
   por design. Um app de captura em Wayland é uma experiência diferente da que
   o RustShot tem hoje, não a mesma noutro sistema.

2. **OCR no Linux.** Não existe motor do sistema. A única saída realista é o
   Tesseract, que foi rejeitado aqui por pesar ~25 MB e exigir instalação
   separada — exatamente o que a decisão de usar o motor do Windows evitou. Ou
   o Linux sai sem OCR, ou a promessa de "todas com as mesmas funcionalidades"
   não se cumpre nesse ponto.

3. **macOS.** Tecnicamente é o mais tranquilo (Vision para OCR, ScreenCaptureKit
   para captura), mas exige permissões de Gravação de Tela e Acessibilidade, e
   distribuir fora da App Store pede conta de desenvolvedor Apple paga e
   notarização — sem isso o app não abre em máquina alheia.

**Sobre "as mesmas funcionalidades":** dá para chegar perto em macOS. Em Linux,
não sem abrir mão de algo — OCR, ou enumeração de janelas, ou a fluidez do
atalho em Wayland. Prefiro dizer isso agora a descobrir na metade.

**Caminho sugerido, se for adiante:** extrair uma interface de plataforma
(um `trait` por capacidade) com a implementação Windows atual por trás, e só
então escrever a segunda. O núcleo — editor, op-log, rasterizador, imgbuf,
JPEG — já é portátil e não precisa mudar: são as ~15 caixinhas da tabela acima
que dão o trabalho, duas vezes.

---

## Ordem sugerida

Se a ideia é ganhar mais com menos, nesta ordem:

1. **Fixar na tela** (§6) — o mais pedido, esforço moderado, sem armadilha.
2. **PNG e escolha automática de formato** (§7) — corrige o pior defeito da
   saída atual para o caso mais comum.
3. **QR code** (§5) — barato e distintivo.
4. **Régua e cor média** (§3) — vários itens P que somam.
5. **Seta em arco e estilo à mão** (§2) — o que mais muda a aparência do
   resultado.
6. **Captura com rolagem** (§1) — o maior, e o que mais diferencia.
7. **Multiplataforma** (§8) — depois de tudo, e com as ressalvas acima.
