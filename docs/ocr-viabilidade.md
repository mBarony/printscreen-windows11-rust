# OCR no RustShot — viabilidade, protótipo e o que o PowerToys ensina

**Data:** 22–23/08/2026 · **Branch:** `worktree-ocr-teste` · **Base:** v1.6.0

Investigação sobre acrescentar reconhecimento de texto ao RustShot: se cabe na
arquitetura, quanto custa no binário, e o que aproveitar do
[PowerToys](https://github.com/microsoft/PowerToys), que resolve o mesmo
problema no mesmo sistema operacional.

O OCR havia sido posto **fora de escopo** no port do omasnap. Esta investigação
reabre a questão porque a premissa que sustentava aquela decisão estava errada
— ver a seção seguinte.

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
| Features WinRT na crate `windows` (já presente) | ~2,5 MB de bitcode antes do LTO; 200 linhas de Rust seguro |
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
entrada de menu — é a prova de conceito que permite medir o custo real. O
`#![allow(dead_code)]` no topo é temporário e sai junto com o botão.

---

## 3. O que foi verificado

Nesta máquina (macOS, sem linker MSVC):

| Verificação | Resultado |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | limpo |
| `cargo clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings` | limpo |
| `cargo test` | **205 aprovados**, 1 ignorado (era 199 antes) |
| `cargo build --release --target x86_64-pc-windows-msvc` | compila; falha só no **link** (sem MSVC no macOS) |

Os 5 testes novos cobrem a ampliação bilinear + conversão RGBA→BGRA, que é
lógica pura e roda em qualquer plataforma: troca de canais, dimensões de saída,
preservação dos cantos, existência de tons intermediários na transição (uma
ampliação por vizinho mais próximo não os teria) e o caso degenerado de 1 px.

O que **não** dá para verificar daqui: que o reconhecimento funciona de fato.
Isso exige um Windows com pacote de idioma instalado.

---

## 4. Custo no binário

O número que interessa é quanto o `rustshot.exe` cresce. Ele **não** foi medido
— exige o linker MSVC. O que dá para medir aqui são os artefatos intermediários,
e eles precisam ser lidos com cuidado.

Compilando a crate `windows` em release, com e sem as features de OCR:

| | rlib | rmeta | código (rlib − rmeta) |
|---|---:|---:|---:|
| baseline (21 features, o que o wgpu pede) | 24.334 KB | 23.221 KB | 1.113 KB |
| com OCR (30 features) | 32.129 KB | 28.485 KB | 3.644 KB |
| **delta** | **7.795 KB** | 5.264 KB | **2.531 KB** |

O delta de 7,8 MB no rlib assusta e é enganoso: **5,3 MB dele é `rmeta`** —
metadata que o rustc usa para compilar quem depende da crate e que **não entra
no executável**. O que pode chegar ao exe é o resto: **~2,5 MB**.

E mesmo esse 2,5 MB é um **limite superior**, não uma estimativa. O perfil de
release usa `lto = "fat"` + `codegen-units = 1`: o linker vê o bitcode do
programa inteiro e descarta tudo que não é alcançável. O `ocr.rs` toca cinco
tipos (`OcrEngine`, `SoftwareBitmap`, `Language`, `DataWriter`, `OcrResult`) de
um feature set que traz milhares. A fatia que sobrevive deve ser uma fração
pequena disso, mas **fração desconhecida** — o LTO é a única autoridade sobre
esse número, e ele só roda com o linker.

Contexto: o exe hoje tem ~5,6 MB contra o alvo de 15 MB do CI (RNF-01). Mesmo
no pior caso — os 2,5 MB inteiros sobrevivendo ao LTO — ainda haveria ~6,9 MB
de folga. **O risco de estourar o alvo é baixo**, mas a medida real continua
pendente.

### Como medir (numa máquina Windows)

Com o branch `worktree-ocr-teste` em mãos:

```powershell
# 1. Baseline: comente o bloco `windows = { ... }` no Cargo.toml
#    e o `pub mod ocr;` em src/platform/mod.rs
cargo build --release
"{0:N2} MB" -f ((Get-Item .\target\release\rustshot.exe).Length / 1MB)

# 2. Descomente os dois e repita
cargo build --release
"{0:N2} MB" -f ((Get-Item .\target\release\rustshot.exe).Length / 1MB)
```

A diferença entre as duas linhas é a resposta. Com ela em mãos, a decisão de
seguir ou não com o OCR deixa de ser especulativa.

Aproveitando a máquina, vale confirmar que o reconhecimento funciona:
`available_languages()` deve listar ao menos um idioma, e `recognize()` sobre
uma captura de texto deve devolvê-lo com as quebras de linha no lugar.

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
passar por configuração. O protótipo usa `TryCreateFromUserProfileLanguages`,
que respeita a lista de idiomas preferidos do perfil — decente por padrão, mas
menos esperto nesse caso. `GetKeyboardLayout(0)` (Win32, já disponível) daria o
mesmo comportamento aqui; fica como opção quando houver interface para expor a
escolha.

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

O que mudou desde que o OCR saiu do escopo:

1. A dependência que parecia proibitiva **já estava no binário**. O argumento
   que fechou a questão não se sustenta.
2. O protótipo existe, compila limpo para `x86_64-pc-windows-msvc`, e as 205
   verificações passam.
3. O custo no exe tem **limite superior de ~2,5 MB** contra ~9,4 MB de folga
   até o alvo do CI — provavelmente muito menos, depois do LTO.
4. A implementação de referência da Microsoft confirma a API e entregou um
   truque concreto (a ampliação de 1,5×) que já está aplicado.

O que falta antes de decidir:

- **Medir o exe** numa máquina Windows (comando na seção 4).
- **Confirmar que reconhece** — nenhum teste aqui exercita o motor de verdade.

Se o delta do exe vier abaixo de ~1 MB, o argumento contra o OCR fica difícil de
sustentar: é uma funcionalidade que a Ferramenta de Captura tem, que o omasnap
tem, e que a essa altura custaria pouco mais que o botão para acioná-la.

A ligação com a interface — atalho, entrada de menu, para onde vai o texto — não
foi desenhada e não é objeto desta investigação.
