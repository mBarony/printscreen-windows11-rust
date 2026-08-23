# OCR no RustShot — viabilidade, protótipo e o que o PowerToys ensina

**Data:** 22–23/08/2026 · **Branch:** `worktree-ocr-teste` · **Base:** v1.6.0

Investigação sobre acrescentar reconhecimento de texto ao RustShot: se cabe na
arquitetura, quanto custa no binário, e o que aproveitar do
[PowerToys](https://github.com/microsoft/PowerToys), que resolve o mesmo
problema no mesmo sistema operacional.

O OCR havia sido posto **fora de escopo** no port do omasnap. Esta investigação
reabre a questão porque a premissa que sustentava aquela decisão estava errada
— ver a seção seguinte.

**Em uma linha:** custa **14,5 KiB** no executável (0,1% do orçamento do CI),
funciona, e a dependência que parecia proibitiva já estava no binário.

---

## 1. A correção que motivou tudo

Ao planejar o port do omasnap, registrei que OCR exigiria **adicionar a crate
`windows` à árvore de dependências**, contra a filosofia standalone do projeto
(um exe, Win32 direto via `windows-sys`, sem crates de conveniência). Foi com
esse custo na mesa que o OCR saiu do escopo.

**A premissa estava errada.** A crate `windows` já está no binário hoje:

```
windows v0.58.0
├── gpu-allocator v0.27.0
│   └── wgpu-hal v25.0.2
│       ├── wgpu v25.0.2
│       │   └── eframe v0.32.3 → rustshot
```

O backend DX12 do wgpu a arrasta por `gpu-allocator`. Ela é compilada e linkada
em todo `rustshot.exe` desde a v1.3, quando o renderizador passou de glow para
wgpu. Habilitar `Media_Ocr` **não adiciona crate nenhuma** — soma features a uma
crate que já está lá, e as features dessa crate são granulares (um `#[cfg]` por
submódulo do Windows SDK).

Isso não torna o OCR gratuito, mas muda a natureza da decisão: não é mais
"aceitar uma dependência nova", é "quanto código a mais o linker vai incluir".
Que é uma pergunta com resposta numérica — a seção 4.

### Por que não `windows-sys`

A regra do projeto continua valendo: `windows-sys` para Win32 clássico. Mas
`Windows.Media.Ocr` é **WinRT**, e o `windows-sys` cobre apenas Win32 — não há
feature a habilitar, o módulo simplesmente não existe lá. As alternativas eram:

| Caminho | Custo |
|---|---|
| Features WinRT na crate `windows` (já presente) | **14,5 KiB** no exe; 200 linhas de Rust seguro |
| Declarar as vtables COM à mão sobre `windows-sys` | ~450 linhas de `unsafe`, `IInspectable`/`IActivationFactory`/`IAsyncOperation` reimplementados, para economizar alguns KB |
| Tesseract | +25 MB, instalação separada, mata a proposta de um exe só |

A primeira é a única que não briga com o resto do projeto. A regra fica:
`windows-sys` para Win32 clássico, `windows` só onde a API é WinRT — hoje isso
quer dizer **só o OCR**.

---

## 2. O protótipo

`src/platform/ocr.rs`, ~200 linhas, duas funções públicas:

```rust
pub fn recognize(image: &RgbaImage, language: Option<&str>) -> Result<String>
pub fn available_languages() -> Vec<String>
```

Decisões que valem registro:

- **Bloqueante por contrato.** A API WinRT é assíncrona; `RecognizeAsync().get()`
  bloqueia a thread. O módulo documenta que só pode ser chamado de thread de
  trabalho (`crate::jobs`), nunca da thread da interface. Um OCR de tela cheia
  leva centenas de milissegundos — na thread da UI seria um congelamento visível.
- **`Lines()`, não `Text()`.** `OcrResult::Text` devolve tudo numa linha só.
  Percorrer `Lines()` preserva as quebras, que é o que faz o texto colado
  continuar legível. Mesma escolha do PowerToys.
- **`SoftwareBitmap::CreateCopyFromBuffer`** direto do buffer BGRA, sem passar
  por codificação BMP intermediária (o PowerToys passa — seção 5.2).
- **Erros com saída acionável.** Quando falta o pacote de idioma, a mensagem diz
  onde instalá-lo (Configurações › Hora e idioma › Idioma) em vez de repassar o
  `HRESULT` cru.
- **Guarda de `MaxImageDimension`** antes de chamar o motor, para dar erro
  legível em vez do erro cru do WinRT.

O motor é o mesmo da Ferramenta de Captura do Windows: funciona num Windows 11
limpo, sem o usuário instalar nada.

**O módulo ainda não está ligado à interface.** Não há botão, atalho nem
entrada de menu. O único acionador é a flag de linha de comando:

```
rustshot --ocr <imagem>
```

que abre a imagem pelo GDI+ (`platform::imagefile`, já existente) e mostra o
texto reconhecido. Ela não nasceu como funcionalidade e sim como necessidade de
medição — sem nenhum caminho que chegue ao módulo a partir do `main`, o LTO o
descarta e não há o que medir (seção 4.2).

Ele fica atrás da feature de cargo **`ocr`**, fora do padrão: o build normal
continua idêntico ao de hoje, e a comparação de tamanho vira `cargo build
--release` contra `cargo build --release --features ocr`, sem editar arquivo.

---

## 3. O que foi verificado

Localmente (macOS, sem linker MSVC):

| Verificação | Resultado |
|---|---|
| `cargo clippy --all-targets -- -D warnings` (com e sem `--features ocr`) | limpo |
| `cargo clippy --all-targets --target x86_64-pc-windows-msvc` (idem) | limpo |
| `cargo test --features ocr` | **206 aprovados**, 1 ignorado (eram 199 antes) |
| `cargo test` (sem a feature) | 201 aprovados |
| `cargo build --release --target x86_64-pc-windows-msvc` | compila; falha só no **link** (sem MSVC no macOS) |

No Windows a contagem de ignorados é 2, não 1: `ocr_de_verdade` está sob
`#[cfg(windows)]` e no macOS nem chega a existir, sobrando só o `svg_preview`
do `editor::icons`.

No CI (`windows-latest`): build, testes, clippy, tamanho do exe nas duas
configurações e o teste de reconhecimento — tudo verde.

Os 7 testes novos: 5 cobrem a ampliação bilinear + conversão RGBA→BGRA (troca
de canais, dimensões de saída, preservação dos cantos, existência de tons
intermediários na transição — uma ampliação por vizinho mais próximo não os
teria — e o caso degenerado de 1 px); 2 cobrem o parsing de `--ocr`. Mais o
`ocr_de_verdade`, ignorado por padrão, que exercita o motor.

### Nota: o CI estava vermelho, e não por causa disto

Ao abrir o CI para medir, encontrei-o falhando desde 22/08 — inclusive na
`main`. A causa não era o OCR nem o port: o clippy do **Rust 1.98** no runner
passou a exigir `as_chunks` onde o tamanho do bloco é constante
(`chunks_exact_to_as_chunks`), e o toolchain local aqui é 1.97, que não tem
esse lint. O último run verde, de 18/08, tinha exatamente os mesmos laços.

Corrigido nos cinco pontos de conversão de pixel (`imgbuf`, `capture`,
`clipboard`, `shell`, `imagefile`), sem mudança de comportamento — os laços
indexam por posição e `[u8; 4]` indexa igual a um slice. **A correção está
neste branch; a `main` continua vermelha até um merge.**

Fica também o problema estrutural, que a correção não resolve: o `ci.yml` faz
`rustup toolchain install stable` sem pin, e não há `rust-toolchain.toml` nem
`rust-version` no `Cargo.toml`. Um lint novo em qualquer stable futura volta a
quebrar a `main` sozinha, sem ninguém ter tocado em nada. Pinar a toolchain é a
outra metade da solução, e é decisão de projeto — não foi feita aqui.

---

## 4. Custo no binário

**Medido**, no runner `windows-latest` do próprio CI (o passo está no
`ci.yml`; ver seção 4.2 sobre por que não foi numa máquina local):

| | bytes | MB |
|---|---:|---:|
| sem ocr | 6.005.760 | 5,728 |
| com ocr | 6.020.608 | 5,742 |
| **delta** | **14.848** | **0,014** |

**14,5 KiB.** Um quarto de por cento do executável, e 0,1% do orçamento de
15 MB do CI (RNF-01) — que continua com 9,26 MB de folga.

O número foi confirmado por uma segunda medição independente, numa máquina
Windows com MSVC real (Build Tools 2022 17.14.39, SDK 10.0.26100, rustc 1.98)
e com o OCR ligado em **outro** ponto de entrada — dentro de
`run_quick_capture` em vez da flag `--ocr`:

| medição | ponto de entrada | delta |
|---|---|---:|
| CI (`windows-latest`) | `--ocr <imagem>` | 14.848 |
| Windows local, MSVC | `run_quick_capture` | 16.384 |

Dois pontos de entrada diferentes, duas toolchains diferentes, mesma ordem de
grandeza. A diferença de 1.536 bytes são 3 blocos do `FileAlignment` do PE
(512 bytes) e explica-se pelo que cada caminho arrasta: o `--ocr` reusa o
`imagefile`, que já estava no binário, e o outro puxa o logging.

Vale a nota sobre granularidade: o tamanho de um PE no disco é múltiplo do
`FileAlignment`, 512 bytes — não do `SectionAlignment` de 4096. Os 16.384
serem exatamente 4×4096 é coincidência; 14.848 é 29×512 e não é múltiplo de
4096. A quantização é fina o bastante para os dois números serem reais, e não
tetos arredondados.

### 4.1. Por que tão pouco, e por que a estimativa errou por 175×

Antes de medir, o que dava para observar daqui eram os artefatos
intermediários. Compilando a crate `windows` em release, com e sem as features
de OCR:

| | rlib | rmeta | código (rlib − rmeta) |
|---|---:|---:|---:|
| baseline (21 features, o que o wgpu pede) | 24.334 KB | 23.221 KB | 1.113 KB |
| com OCR (30 features) | 32.129 KB | 28.485 KB | 3.644 KB |
| **delta** | **7.795 KB** | 5.264 KB | **2.531 KB** |

Duas correções sucessivas, ambas para baixo:

1. O delta de 7,8 MB no rlib é enganoso: **5,3 MB dele é `rmeta`**, metadata
   que o rustc usa para compilar quem depende da crate e que não entra no
   executável. Sobram 2,5 MB de código — este era o limite superior.
2. Desses 2,5 MB, **99,4% não sobrevive ao LTO**. O perfil de release usa
   `lto = "fat"` + `codegen-units = 1`: o linker vê o bitcode do programa
   inteiro e descarta tudo que não é alcançável. O `ocr.rs` toca cinco tipos
   (`OcrEngine`, `SoftwareBitmap`, `Language`, `DataWriter`, `OcrResult`) de um
   feature set que traz milhares, e é só isso que fica.

A lição vale para além do OCR: **medir rlib é medir a coisa errada.** A
diferença entre o limite superior defensável e o número real foi de 175×, e
nenhum raciocínio sobre os artefatos intermediários teria chegado perto. O LTO
é a única autoridade sobre esse número.

### 4.2. O primeiro resultado foi zero — e era falso

A primeira medição deu delta **exatamente 0**: o exe saiu byte a byte idêntico
nas duas configurações, apesar de o build ter recompilado a crate `windows`
inteira com as features novas (2m07s no log).

A causa: nada no programa chamava `recognize`. O módulo era inalcançável a
partir do `main`, e o LTO o descartava por completo — a medição estava medindo
código morto. Foi o que motivou o `--ocr <imagem>` da seção 2: sem um caminho
que chegue ao módulo, não há o que medir.

O diagnóstico foi confirmado de forma independente numa máquina Windows com
MSVC real, e por um caminho melhor que o meu: lá o zero se repetiu (6.003.712
bytes nas duas configurações, com SHA256 diferentes — mesmo tamanho, conteúdo
diferente), e a busca por `Windows.Media.Ocr`, `OcrEngine` e pela mensagem de
erro do módulo **não os encontrou em nenhum dos dois binários**, nem em ASCII
nem em UTF-16. Eu havia deduzido a ausência pelo tamanho idêntico; procurar os
símbolos prova a mesma coisa diretamente.

Fica como armadilha registrada: num binário com LTO fat, **acrescentar uma
dependência e medir o exe não diz nada enquanto ninguém a usar**. O zero parece
uma ótima notícia e é só ausência de código.

### 4.3. Como reproduzir

O OCR está atrás da feature de cargo **`ocr`**, fora do padrão justamente para
que a medida não dependa de editar arquivo nenhum:

```powershell
cargo build --release
(Get-Item .\target\release\rustshot.exe).Length

cargo build --release --features ocr
(Get-Item .\target\release\rustshot.exe).Length
```

O CI faz exatamente isso a cada push e publica a tabela no resumo do run —
não é preciso máquina local. A medida acima veio de lá.

---

## 4.4. O motor reconhece

Confirmado no mesmo run:

```
idiomas com pacote de OCR: ["en-US"]
--- reconhecido ---
RUSTSHOT
--- fim ---
test platform::ocr::tests::ocr_de_verdade ... ok
```

O teste rasteriza `RUSTSHOT` com a fonte da exportação e confere que o motor o
devolve. A pipeline inteira — ampliação bilinear → BGRA → `SoftwareBitmap` →
`OcrEngine` → `Lines` — funciona de ponta a ponta num Windows limpo, com o
único pacote de idioma que o runner traz de fábrica.

Rasterizar em vez de capturar a tela foi uma correção de rota: a primeira
versão do teste lia o monitor primário, o que o tornava dependente do que
estivesse aberto e **despejava o conteúdo da tela de quem o rodasse na saída
do CI**. A versão atual é determinística e não lê nada da máquina.

### 4.3. Numa máquina real, e o bug que ela revelou

O teste foi enfim rodado fora de runner: Windows 11 Pro 22631 **em pt-BR**,
rustc 1.98.0, MSVC 14.44. O motor reconhece — `RUSTSHOT` voltou exato — mas o
caminho até ele estava quebrado, e de um jeito que nenhum CI pegaria:

```
idiomas com pacote de OCR: ["en-US"]
reconhecimento falhou: não foi possível iniciar o OCR:
  The operation completed successfully. (0x00000000)
```

A máquina tem o Windows em pt-BR e só o pacote de OCR **en-US** instalado.
Quando nenhum idioma do perfil tem pacote, `TryCreateFromUserProfileLanguages`
devolve **nulo** — e não erro. A windows-rs converte esse nulo num `Error` cujo
`HRESULT` é `S_OK`, e formatá-lo produz "The operation completed successfully".

O resultado era o pior dos mundos: **o OCR falhava numa máquina com motor
utilizável**, anunciando sucesso na mensagem de erro. A decisão de projeto
"erros com saída acionável" (seção 2) não se sustentava neste caminho.

O CI não pega isso por construção. O runner do `windows-latest` está em en-US,
o perfil casa com o pacote instalado, a chamada funciona e ninguém nota. É
preciso um Windows num idioma **sem** pacote de OCR — configuração comum entre
usuários reais, e ausente de qualquer runner.

A correção é a que a seção 5.3 já apontava no PowerOCR: recuar para o primeiro
pacote instalado quando o perfil não serve, com mensagem útil quando não há
pacote nenhum. Está em `default_engine()`, com o comentário sobre a armadilha
do `S_OK` nulo — sem ele o próximo leitor reintroduz o bug.

Continua por confirmar: reconhecimento de **texto em português**, com acentos e
com o pacote pt-BR instalado. O que ficou provado aqui é que a pipeline
funciona e que o recuo de idioma funciona; esta máquina não tem o pacote pt-BR.

---

## 5. O que o PowerToys ensina

O módulo **PowerOCR** (`src/modules/PowerOCR/`, 31 arquivos, ~3.500 linhas C#,
licença MIT) é a implementação de referência: mesmo sistema operacional, mesmo
problema, em produção há anos e com um volume de usuários que o RustShot não
tem. Ele descende do [Text Grab](https://github.com/TheJoeFin/Text-Grab), o que
explica a string `"Text Grab"` que ainda aparece em algumas caixas de mensagem.

### 5.1. O que ele valida

O PowerOCR usa **exatamente a mesma API** que escolhi:

```csharp
OcrEngine ocrEngine = OcrEngine.TryCreateFromLanguage(selectedLanguage);
OcrResult ocrResult = await ocrEngine.RecognizeAsync(softwareBmp);
```
> `Helpers/ImageMethods.cs:157-158`

E percorre `ocrResult.Lines` em vez de usar `.Text`, pela mesma razão. Que a
Microsoft resolva assim no próprio produto é a melhor confirmação disponível de
que `Windows.Media.Ocr` é o caminho certo no Windows 11 — não há motor melhor
escondido, nem armadilha conhecida que justifique Tesseract.

### 5.2. O que foi adotado daqui

**A ampliação de 1,5× antes de reconhecer.** É o achado de maior valor prático,
e não é óbvio:

```csharp
if (bmp.Width * 1.5 > OcrEngine.MaxImageDimension) scaleBMP = false;
using Bitmap scaledBitmap = scaleBMP ? ScaleBitmapUniform(bmp, 1.5)
                                     : ScaleBitmapUniform(bmp, 1.0);
```
> `Helpers/OcrExtensions.cs:77-82` e `Helpers/ImageMethods.cs:138-146`

O motor foi treinado para texto de documento digitalizado. Fonte de interface
numa captura 1:1 fica pequena demais para ele, e o acerto cai sensivelmente.
Ampliar antes resolve, e é barato perto do custo do próprio reconhecimento.
O detalhe que completa o truque é o **guarda**: se 1,5× estourar
`MaxImageDimension`, vai 1:1 em vez de falhar.

Já implementado em `ocr.rs`, com a mesma constante e o mesmo guarda. A
interpolação bilinear e a conversão RGBA→BGRA acontecem no mesmo laço — o
`SoftwareBitmap` quer BGRA de qualquer jeito, então a troca de canais sai de
graça junto da reamostragem.

Uma diferença deliberada: o PowerToys serializa o bitmap para BMP em memória e
o passa por `BitmapDecoder.CreateAsync` (`ImageMethods.cs:149-155`) — codifica
e decodifica só para chegar ao `SoftwareBitmap`. Idiomático em C#/GDI+; aqui,
com os pixels já num `Vec<u8>`, `CreateCopyFromBuffer` chega ao mesmo lugar sem
o round-trip.

### 5.3. O que vale adotar quando o OCR for para a interface

**Idiomas sem espaço entre palavras.** Chinês e japonês não separam palavras por
espaço; juntar os `OcrWord` com espaço, como se faz em português, produz lixo:

```csharp
if (LanguageTag.StartsWith("zh")) return false;
if (LanguageTag.Equals("ja"))     return false;
return true;
```
> `Helpers/LanguageHelper.cs:13-25`

E quando o idioma **não** é space-joining, o PowerToys não junta tudo cru: usa a
regex `(^[\p{L}-[\p{Lo}]]|\p{Nd}$)|.{2,}` para decidir palavra a palavra se ela
merece espaço antes — caracteres CJK são "outras letras" (`\p{Lo}`) e palavras
de um caractere só, então pontuação e símbolos soltos grudam sem espaço
(`Helpers/OcrExtensions.cs:34-63`). É o tipo de detalhe que só aparece depois
de muitos relatos de usuário.

Hoje o `ocr.rs` sempre junta as **linhas** com `\n` e usa o `Text()` de cada
linha, que já vem com o espaçamento do motor — o problema não chega a se
manifestar. Se algum dia o texto for montado palavra a palavra (para respeitar
colunas, por exemplo), essa regra passa a ser necessária.

**A escolha do idioma padrão.** O PowerToys **não** usa
`TryCreateFromUserProfileLanguages`. Ele parte do **idioma do teclado ativo**:

```csharp
string inputLang = InputLanguageManager.Current.CurrentInputLanguage.Name;
// ... se não houver pacote exato, tenta por AbbreviatedName,
//     e por fim o primeiro instalado
```
> `Helpers/ImageMethods.cs:274-302`

É mais reativo que o perfil do usuário: quem tem o Windows em inglês mas digita
em português acerta, e trocar o layout do teclado troca o idioma do OCR sem
passar por configuração. O protótipo partia de
`TryCreateFromUserProfileLanguages` e parava aí — o que se mostrou um bug de
verdade, não uma imperfeição: sem o recuo, um Windows em pt-BR com pacote
en-US instalado falha tendo motor à mão (seção 4.3). Hoje há o recuo para o
primeiro pacote instalado, que é o último degrau do PowerToys.

Falta o degrau do meio: o idioma do **teclado**. `GetKeyboardLayout(0)` (Win32,
já disponível) daria o mesmo comportamento aqui, e é o que acerta para quem tem
o Windows em inglês mas digita em português. Fica como opção quando houver
interface para expor a escolha.

**Reconstrução de tabelas.** `Models/ResultTable.cs` (644 linhas) reconstrói
linhas e colunas a partir dos `BoundingRect` das palavras: projeta as caixas
numa grade, encontra as faixas vazias e usa os pontos médios entre elas como
divisórias (`ResultTable.cs:85-101`). É a resposta ao problema clássico do OCR
de captura de tela — texto em colunas sai embaralhado, porque o motor lê por
linha visual atravessando as colunas.

Não é para agora: são 644 linhas para um caso de uso específico. Mas é a prova
de que o `OcrResult` carrega geometria suficiente para resolver o problema, se
ele aparecer. Vale saber que existe antes de alguém tentar reinventá-lo.

### 5.4. O que não serve

- **A arquitetura de janela.** O PowerOCR abre um overlay WPF próprio por
  monitor, com seu ciclo de vida. O RustShot já tem overlay e editor; o OCR
  entra como ação sobre uma imagem que já existe, não como um modo novo.
- **`GC.Collect()` explícito** depois de cada reconhecimento
  (`ImageMethods.cs:162`, `:230`, `:250`, `:270`) — contorno para o consumo de
  memória dos bitmaps GDI+. Sem paralelo em Rust.
- **`WrappingStream`** — classe inteira só para impedir que o
  `BitmapDecoder` feche o `MemoryStream` alheio. Artefato do round-trip por BMP
  que o protótipo não faz.
- **A camada de configurações** (`PowerOcrViewModel` etc.) é ligada ao runner do
  PowerToys. O RustShot tem `config.json` + `settings.rs` próprios.

---

## 6. Onde isto deixa a decisão

As duas premissas que tiraram o OCR do escopo caíram:

1. A dependência que parecia proibitiva **já estava no binário** desde a v1.3.
   Não havia crate nova a aceitar.
2. O custo, que se supunha da ordem de megabytes, é de **14,5 KiB** — 0,1% do
   orçamento de 15 MB. Sobram 9,26 MB de folga.

E o que se sabe hoje que não se sabia:

3. O motor **reconhece**, confirmado no CI, com a pipeline inteira do protótipo.
4. A implementação de referência da Microsoft usa a mesma API e entregou um
   truque concreto — a ampliação de 1,5× —, já aplicado.
5. 206 verificações passam; clippy limpo no host e em
   `x86_64-pc-windows-msvc`, com e sem a feature.

Nada disso obriga a adotar o OCR — é uma decisão de produto, sobre se a
funcionalidade pertence ao RustShot. Mas **o argumento técnico contra ela não
existe mais**: é uma funcionalidade que a Ferramenta de Captura tem, que o
omasnap tem, e que custa 0,25% do executável.

O que continua em aberto:

- **A ligação com a interface** — atalho, entrada de menu, para onde vai o
  texto. Hoje só há `--ocr <imagem>`, que existe para tornar o módulo
  alcançável. Um botão no editor é o passo natural, e não foi desenhado.
- **Copiar o texto reconhecido** exigiria um `set_text` no
  `platform::clipboard`, que hoje só escreve imagem (`CF_UNICODETEXT` em vez de
  `CF_DIB`; poucas linhas, mas não escritas).
- **Reconhecimento em pt-BR**, que nenhum runner pode confirmar.
- Se o OCR for adotado, sai o `#![allow(dead_code)]` do módulo e a feature
  provavelmente deixa de fazer sentido como opcional.
