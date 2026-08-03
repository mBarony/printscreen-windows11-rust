# Binário pré-compilado

`rustshot.exe` — RustShot v1.3.0 para Windows 11 x64, pronto para uso: baixe,
coloque em uma pasta gravável e execute. O `config.json` e o `rustshot.log`
são criados ao lado do executável.

**Download direto:**
<https://github.com/mBarony/printscreen-windows11-rust/raw/main/dist/rustshot.exe>

## Procedência deste binário

| | |
|---|---|
| Versão | 1.3.0 (VersionInfo embutido) |
| Alvo | `x86_64-pc-windows-gnu` (compilado de forma cruzada com MinGW-w64) |
| Tamanho | 8,13 MB (alvo RNF-01: ≤ 15 MB) |
| SHA-256 | `bf7c54f2f919b391d8948a35a0692ac8ccff039e713e046daa63b443a2d7d255` |

Verificado no PE: subsistema **GUI** (sem janela de console), manifesto
**DPI Per-Monitor V2**, ícone e VersionInfo embutidos, e importações apenas de
DLLs do próprio Windows — **não precisa de runtime do MinGW nem de nenhum
outro pacote instalado**.

## Qual binário usar

Este arquivo é um build **de conveniência**, com a ABI GNU. O build canônico
do projeto é o **MSVC** (`stable-x86_64-pc-windows-msvc`), que é o que o CI
compila e testa a cada commit. Se você quer exatamente o binário validado
pelo CI, prefira uma destas rotas:

- **Artefato do CI**: aba *Actions* → run mais recente da `main` →
  *Artifacts* → `rustshot-windows-x64`.
- **Compilar você mesmo**: `.\build.ps1` na raiz do repositório (roda clippy,
  testes e o build release MSVC).

As duas rotas produzem o mesmo aplicativo; a diferença está no runtime C
usado pelo compilador, não no comportamento do RustShot.
