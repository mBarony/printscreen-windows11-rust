#Requires -Version 5.1
<#
.SYNOPSIS
    Compila o RustShot localmente no Windows e prepara o exe para uso.

.DESCRIPTION
    Roda o que o CI roda (clippy com -D warnings, testes, build release) e mais
    o que so' importa localmente: valida o toolchain MSVC, mede o artefato
    contra o alvo de 15 MB (RNF-01), confere o VersionInfo do exe e,
    opcionalmente, instala o resultado em uma pasta e o executa.

    Trata o caso do Smart App Control: quando um artefato intermediario e'
    bloqueado por reputacao de hash (os error 4551 / E0463), o script explica
    e oferece re-rolar o rotulo -Cmetadata de .cargo\config.toml (-NewSalt),
    que e' a saida correta — nunca desativar o SAC.

.PARAMETER Dev
    Compila o perfil dev em vez de release (build rapido, log em nivel Debug).

.PARAMETER SkipChecks
    Pula clippy e testes; so' compila. Util para iterar rapido.

.PARAMETER Run
    Executa o rustshot.exe ao final (encerra a instancia anterior, se houver).

.PARAMETER InstallTo
    Copia o exe para esta pasta ao final (ex.: "$env:LOCALAPPDATA\RustShot").
    Lembre-se: o app guarda config.json e rustshot.log ao lado do executavel.

.PARAMETER NewSalt
    Re-rola o rotulo -Cmetadata em .cargo\config.toml antes de compilar
    (sacfresh3 -> sacfresh4 -> ...). Use quando o SAC bloquear o build.

.PARAMETER Clean
    Roda 'cargo clean' antes de compilar.

.EXAMPLE
    .\build.ps1
    Verificacao completa + build release.

.EXAMPLE
    .\build.ps1 -SkipChecks -Run
    Compila e ja' abre o app (iteracao rapida).

.EXAMPLE
    .\build.ps1 -InstallTo "$env:LOCALAPPDATA\RustShot" -Run
    Compila, instala na pasta do usuario e executa de la'.
#>

[CmdletBinding()]
param(
    [switch]$Dev,
    [switch]$SkipChecks,
    [switch]$Run,
    [string]$InstallTo,
    [switch]$NewSalt,
    [switch]$Clean
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = $PSScriptRoot

function Write-Step([string]$Text) {
    Write-Host ''
    Write-Host "==> $Text" -ForegroundColor Cyan
}

function Write-Ok([string]$Text) {
    Write-Host "    OK: $Text" -ForegroundColor Green
}

function Write-Warn2([string]$Text) {
    Write-Host "    Aviso: $Text" -ForegroundColor Yellow
}

function Write-Err2([string]$Text) {
    Write-Host "    ERRO: $Text" -ForegroundColor Red
}

# Roda o cargo, ecoa a saida e devolve (exit code + texto) para inspecao.
# O cargo escreve todo o progresso em stderr; com ErrorActionPreference='Stop'
# o PowerShell 7 transforma isso em erro terminante, entao o preference e'
# afrouxado durante a chamada e restaurado logo depois.
function Invoke-Cargo {
    param(
        [Parameter(Mandatory)][string]$Title,
        [Parameter(Mandatory)][string[]]$Arguments
    )

    Write-Step $Title
    Write-Host "    cargo $($Arguments -join ' ')" -ForegroundColor DarkGray

    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $lines = @()
    $code = 0
    try {
        $lines = & cargo @Arguments 2>&1 | ForEach-Object {
            $line = "$_"
            Write-Host "    $line"
            $line
        }
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }

    return [pscustomobject]@{
        ExitCode = $code
        Text     = ($lines -join "`n")
    }
}

function Assert-Success {
    param(
        [Parameter(Mandatory)][psobject]$Result,
        [Parameter(Mandatory)][string]$What
    )

    if ($Result.ExitCode -eq 0) {
        return
    }

    # Smart App Control avalia cada artefato de build por hash, e o veredito
    # e' permanente: repetir o build nao resolve, mudar o -Cmetadata resolve.
    if ($Result.Text -match 'os error 4551' -or $Result.Text -match 'E0463') {
        Write-Err2 "$What falhou — parece bloqueio do Smart App Control."
        Write-Host ''
        Write-Host '    O SAC avalia cada artefato de build por hash e o veredito e permanente.' -ForegroundColor Yellow
        Write-Host '    Solucao: re-rolar o rotulo -Cmetadata para gerar hashes novos:' -ForegroundColor Yellow
        Write-Host '        .\build.ps1 -NewSalt' -ForegroundColor White
        Write-Host '    Nao desative o Smart App Control por causa disso.' -ForegroundColor Yellow
    } else {
        Write-Err2 "$What falhou (exit $($Result.ExitCode))."
    }
    exit 1
}

Push-Location $RepoRoot
try {
    # -----------------------------------------------------------------------
    # Ambiente
    # -----------------------------------------------------------------------

    Write-Step 'Verificando ambiente'

    # $IsWindows so' existe no PowerShell 6+; no Windows PowerShell 5.1 a
    # ausencia da variavel ja' significa Windows.
    $onWindows = $true
    if (Test-Path 'Variable:\IsWindows') {
        $onWindows = $IsWindows
    }
    if (-not $onWindows) {
        Write-Err2 "Este script compila o rustshot.exe e precisa rodar no Windows."
        Write-Host '    Em Linux/macOS da para validar tipos e lints sem linkar:' -ForegroundColor Yellow
        Write-Host '        cargo clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings' -ForegroundColor White
        exit 1
    }

    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Err2 'cargo nao encontrado no PATH. Instale o Rust em https://rustup.rs'
        exit 1
    }

    Write-Ok ((& rustc --version) -join '')

    $hostLine = (& rustc -vV) | Where-Object { $_ -like 'host:*' } | Select-Object -First 1
    $hostTriple = "$hostLine" -replace '^host:\s*', ''
    Write-Ok "host: $hostTriple"

    if ($hostTriple -notlike '*-pc-windows-msvc') {
        Write-Warn2 "toolchain nao e MSVC ($hostTriple)"
        Write-Host '    O projeto e testado em stable-x86_64-pc-windows-msvc:' -ForegroundColor Yellow
        Write-Host '        rustup default stable-x86_64-pc-windows-msvc' -ForegroundColor White
    }

    if (-not $SkipChecks -and -not (Get-Command cargo-clippy -ErrorAction SilentlyContinue)) {
        if (Get-Command rustup -ErrorAction SilentlyContinue) {
            Write-Warn2 'clippy ausente; instalando o componente'
            # Preference afrouxado: no PowerShell 7.4+ um comando nativo com
            # exit code != 0 vira excecao terminante sob 'Stop'.
            $previous = $ErrorActionPreference
            $ErrorActionPreference = 'Continue'
            try {
                & rustup component add clippy
            } finally {
                $ErrorActionPreference = $previous
            }
            if ($LASTEXITCODE -ne 0) {
                Write-Warn2 'nao foi possivel instalar o clippy; use -SkipChecks para pular'
            }
        } else {
            Write-Warn2 'clippy ausente e rustup nao encontrado; use -SkipChecks para pular'
        }
    }

    # -----------------------------------------------------------------------
    # Smart App Control: re-rolar o -Cmetadata quando pedido
    # -----------------------------------------------------------------------

    if ($NewSalt) {
        Write-Step 'Re-rolando o rotulo -Cmetadata (.cargo\config.toml)'
        $configPath = Join-Path $RepoRoot '.cargo\config.toml'
        $content = Get-Content -Path $configPath -Raw
        if ($content -match 'sacfresh(\d+)') {
            $current = [int]$Matches[1]
            $next = $current + 1
            $content = $content -replace "sacfresh$current", "sacfresh$next"
            Set-Content -Path $configPath -Value $content -NoNewline
            Write-Ok "sacfresh$current -> sacfresh$next (os hashes de todos os artefatos mudam)"
            Write-Warn2 'commite essa mudanca se o build passar a funcionar com ela'
        } else {
            Write-Warn2 "rotulo 'sacfreshN' nao encontrado em .cargo\config.toml; nada a fazer"
        }
    }

    if ($Clean) {
        $result = Invoke-Cargo -Title 'Limpando artefatos' -Arguments @('clean')
        Assert-Success -Result $result -What 'cargo clean'
    }

    # -----------------------------------------------------------------------
    # Verificacoes (as mesmas do CI)
    # -----------------------------------------------------------------------

    if ($SkipChecks) {
        Write-Step 'Verificacoes puladas (-SkipChecks)'
    } else {
        $result = Invoke-Cargo -Title 'Clippy (-D warnings, como no CI)' `
                               -Arguments @('clippy', '--all-targets', '--', '-D', 'warnings')
        Assert-Success -Result $result -What 'clippy'
        Write-Ok 'sem warnings'

        $result = Invoke-Cargo -Title 'Testes de unidade' -Arguments @('test')
        Assert-Success -Result $result -What 'cargo test'
        $passed = '?'
        if ($result.Text -match 'test result: ok\. (\d+) passed') {
            $passed = $Matches[1]
        }
        Write-Ok "$passed testes aprovados"
    }

    # -----------------------------------------------------------------------
    # Build
    # -----------------------------------------------------------------------

    $profileName = if ($Dev) { 'dev' } else { 'release' }
    $profileDir = if ($Dev) { 'debug' } else { 'release' }

    $buildArgs = @('build')
    if (-not $Dev) {
        $buildArgs += '--release'
    }

    $result = Invoke-Cargo -Title "Compilando (perfil $profileName)" -Arguments $buildArgs
    Assert-Success -Result $result -What 'cargo build'

    $exePath = Join-Path $RepoRoot "target\$profileDir\rustshot.exe"
    if (-not (Test-Path $exePath)) {
        Write-Err2 "build terminou sem erro, mas $exePath nao existe"
        exit 1
    }

    # -----------------------------------------------------------------------
    # Relatorio do artefato
    # -----------------------------------------------------------------------

    Write-Step 'Artefato'

    $exe = Get-Item $exePath
    $sizeMb = [math]::Round($exe.Length / 1MB, 2)
    Write-Host "    Caminho: $($exe.FullName)"
    Write-Host "    Tamanho: $sizeMb MB"

    if (-not $Dev) {
        if ($exe.Length -le 15MB) {
            Write-Ok 'dentro do alvo de 15 MB (RNF-01)'
        } else {
            Write-Warn2 'acima do alvo de 15 MB (RNF-01)'
        }
    }

    $fileVersion = $exe.VersionInfo.FileVersion
    if ([string]::IsNullOrWhiteSpace($fileVersion)) {
        Write-Warn2 'exe sem VersionInfo — os recursos Win32 (icone/manifesto) podem nao ter sido embutidos'
    } else {
        Write-Host "    Versao:  $fileVersion ($($exe.VersionInfo.ProductName))"
        $versionMatch = Select-String -Path (Join-Path $RepoRoot 'Cargo.toml') `
                                      -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
        if ($versionMatch) {
            $cargoVersion = $versionMatch.Matches[0].Groups[1].Value
            if ($fileVersion.StartsWith($cargoVersion)) {
                Write-Ok "VersionInfo em dia com o Cargo.toml ($cargoVersion)"
            } else {
                Write-Warn2 "VersionInfo ($fileVersion) difere do Cargo.toml ($cargoVersion) — atualize assets\rustshot.rc"
            }
        }
    }

    # -----------------------------------------------------------------------
    # Instalacao e execucao (opcionais)
    # -----------------------------------------------------------------------

    $targetExe = $exe.FullName

    if (-not [string]::IsNullOrWhiteSpace($InstallTo)) {
        Write-Step "Instalando em $InstallTo"

        # Um exe em uso nao pode ser sobrescrito: encerra a instancia antes.
        $running = @(Get-Process -Name 'rustshot' -ErrorAction SilentlyContinue)
        if ($running.Count -gt 0) {
            Write-Warn2 "encerrando instancia em execucao (PID $(($running | ForEach-Object { $_.Id }) -join ', '))"
            $running | Stop-Process -Force
            Start-Sleep -Milliseconds 500
        }

        New-Item -ItemType Directory -Path $InstallTo -Force | Out-Null
        Copy-Item -Path $exe.FullName -Destination $InstallTo -Force
        $targetExe = Join-Path $InstallTo 'rustshot.exe'
        Write-Ok "copiado para $targetExe"
        Write-Host '    (config.json e rustshot.log ficam nessa mesma pasta)' -ForegroundColor DarkGray
    }

    if ($Run) {
        Write-Step 'Executando'

        $running = @(Get-Process -Name 'rustshot' -ErrorAction SilentlyContinue)
        if ($running.Count -gt 0) {
            Write-Warn2 'encerrando instancia anterior (instancia unica, RF-08)'
            $running | Stop-Process -Force
            Start-Sleep -Milliseconds 500
        }

        Start-Process -FilePath $targetExe -WorkingDirectory (Split-Path -Parent $targetExe)
        Write-Ok 'RustShot iniciado — o icone esta na bandeja do sistema'
        Write-Host '    Ctrl+PrtScr: tela cheia | Shift+PrtScr: regiao | Ctrl+Shift+PrtScr: editar' -ForegroundColor DarkGray
    }

    Write-Host ''
    Write-Host 'Concluido.' -ForegroundColor Green
} finally {
    Pop-Location
}
