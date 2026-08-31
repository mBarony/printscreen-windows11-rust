# Todo atalho configurável, pressionando a tecla

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** G

## O que é

Qualquer atalho do programa é redefinido pressionando a combinação desejada em
Configurações — inclusive as teclas de seleção de ferramenta (Retângulo, Mover,
Seta…) e inclusive numerais.

## O tamanho real

O inventário do código encontrou **46 atalhos, dos quais 40 são fixos**. O que
já é configurável hoje são só duas ilhas: os **4 atalhos globais** (que já têm
o modo "pressione a combinação" e já aceitam `Digit0`–`Digit9`, F1–F12,
`Space`, `Enter` e pontuação) e as **16 teclas de ferramenta** (que são um
`ComboBox` de `'A'..='Z'`, sem captura e sem numerais).

| Região | Atalhos | Fixos |
|---|---:|---:|
| Editor | 22 + 1 tecla segurada + 4 modificadores de gesto | 21 |
| Overlay de seleção | 6 | 6 |
| Captura fixada na tela | 1 | 1 |
| Aviso do texto reconhecido | 1 | 1 |
| Janela de Configurações | 2 | 2 |
| Globais | 4 | 0 |

Fora do editor **nada** é configurável: `Esc` que cancela a seleção, `Espaço`
que cicla o modo de mira, `Ctrl+A` que pega o monitor inteiro, `Enter` que
confirma a janela realçada, as setas que navegam entre janelas candidatas, o
`Esc` que fecha a captura fixada.

## Como fazer

O trabalho não é a tela de configuração — é o modelo por baixo dela.

**Unificar o formato antes de ampliar.** Já existem dois jeitos incompatíveis
de gravar "uma tecla" no mesmo `config.json`: `HotkeyDef { modifiers, code }`
(`config.rs:150`), com vocabulário W3C `Code`, e `ToolKeysConfig`
(`config.rs:244`), 16 campos com uma letra solta. Acrescentar os 40 atalhos
fixos sem unificar produziria um terceiro formato — e o `HotkeyDef` já é o
formato certo, porque carrega modificadores.

**Três listas de teclas que não coincidem.** O parser (`hotkeys.rs:57`) aceita
mais teclas do que o seletor da UI oferece (`KEY_CHOICES`, `settings.rs:46`,
sem Tab, Delete e setas, e só até F12), que por sua vez oferece mais do que a
captura consegue reconhecer (`egui_key_to_code`, `settings.rs:624`, sem
PrintScreen, ScrollLock, Pause e F13+). Uma lista só, e derivada.

**Dois resolvedores a partir do mesmo `HotkeyDef`.** Os globais viram
virtual-key do Win32 e passam por `RegisterHotKey`; todo o resto é comparado
como `egui::Key` dentro do laço de eventos. Hoje existe código→vk
(`hotkeys.rs:57`) e egui→código (`settings.rs:624`), e falta o inverso dos
dois. São caminhos com propriedades diferentes: o global é exclusivo no sistema
e **pode falhar ao registrar** se outro aplicativo já tomou a combinação; o do
egui nunca falha, mas exige resolver duplicatas em código.

**Teclas que não chegam como tecla.** `Ctrl+C`, `Ctrl+X` e `Ctrl+V` chegam ao
egui como `Event::Copy`/`Cut`/`Paste`, sem tecla nem modificadores — e o código
hoje trata isso de forma desigual (o `Ctrl+C` do editor olha o evento, o
`Ctrl+V` só olha a tecla). O `PrintScreen` não chega ao egui de jeito nenhum. E
o `Ctrl+V` que fecha o aviso de OCR é lido por `GetAsyncKeyState`, porque
aquela janela não tem foco. Qualquer resolvedor genérico precisa dos três
desvios.

## O que impede um mapa plano "combinação → ação"

**Escopo.** O `Esc` do editor tem seis significados em cascata (fecha a caixa
de texto, senão cancela o arrasto, senão descarta o recorte, senão abandona o
laço, senão desfaz a seleção, senão fecha a janela) e ainda um segundo
tratamento noutro arquivo, para quando a caixa de texto está aberta. `Enter` e
as setas do overlay só valem no modo de mira Janela. O modelo precisa de
contexto, ou esses entram como reservados.

**Ordem de avaliação.** `Ctrl+A` e `Ctrl+V` vêm de propósito antes da guarda de
seleção, porque são justamente os atalhos de quem não tem nada selecionado. Um
despacho por tabela precisa preservar essa precedência, que hoje só está escrita
na ordem física das linhas.

**Casos que não são "uma combinação, uma ação".** `Delete` e `Backspace` fazem
a mesma coisa — o modelo tem de aceitar N teclas por ação. `Shift+setas` não é
atalho separado: o Shift troca o passo de 1 px para 10 px. `Ctrl+V` se desdobra
em duas ações conforme o conteúdo da área de transferência. `Ctrl+roda` já é
configurável, mas por um enum de dois valores, e ficaria órfão.

**Colisões que hoje só não acontecem por acidente.** `Alt+H` convive com o `H`
do Marca-texto, `Alt+D` com o `D` do Ocultar e `Alt+R` com o `R` do Retângulo
apenas porque a tecla de ferramenta exige zero modificadores. Solto o usuário
para mapear, essa proteção deixa de bastar: o detector de conflito precisa
comparar o conjunto exato de modificadores, e ganhar noção de escopo — a mesma
tecla pode estar livre num modo e ocupada em outro.

**Tecla seca disputando com digitação.** Onze atalhos são teclas secas, e a
proteção contra roubar a digitação é hoje ad hoc e desigual: as teclas de
ferramenta exigem `!wants_keyboard_input()` **e** `modifiers.is_none()`; o
`Enter` exige só `!typing`; `Esc`, `Delete` e as setas dependem da guarda do
bloco. Permitir mapear qualquer ação para uma letra pura transforma cada uma
dessas guardas em regra explícita.

## Duas coisas para decidir antes de codar

**O que acontece com quem já tem `config.json`.** Não existe migração: o campo
`version` é lido e regravado, mas nunca comparado com nada, e chave desconhecida
é silenciosamente perdida no próximo save, porque o arquivo é reescrito inteiro
a partir do struct. Se o formato de `hotkeys` mudar de forma, os atalhos
personalizados de quem já usa o programa somem **sem aviso**. Ou o `version`
passa a valer alguma coisa, com uma etapa de upgrade explícita, ou o formato
atual fica estável e só ganha chaves novas — que é a única estratégia que o
código de hoje suporta.

**O que é inválido.** Hoje há duas políticas opostas para o mesmo erro: um
`code` inválido num atalho global deixa a ação **sem atalho** e o lixo
permanece no arquivo; uma tecla de ferramenta inválida **cai no padrão**. Uma
delas tem de ganhar.

## Atalhos que não devem ser configuráveis

O `Esc` que cancela o modo "Detectar…" da própria janela de Configurações
(`settings.rs:598`) é a saída de emergência do capturador: se virar
configurável, o usuário pode se trancar dentro do modo de captura. Fica fixo, e
com a justificativa escrita.

## Efeito colateral obrigatório

Nove textos de ajuda trazem as teclas escritas à mão — os tooltips da barra do
editor ("Salvar e fechar (Ctrl+S)", "Desfazer (Ctrl+Z)"…), a barra de dicas do
overlay ("Esc ou botão direito cancela", "Espaço: janela") e o balão de
primeira execução, que cita `Ctrl+PrtScr`. No dia em que os atalhos mudarem,
todos passam a mentir. O molde certo já existe no próprio arquivo: `tool_hint`
(`toolbar.rs:44`) monta o texto a partir da tecla resolvida, e trata o caso
"sem atalho".
