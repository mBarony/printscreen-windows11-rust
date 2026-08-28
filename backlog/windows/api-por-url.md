# API por esquema de URL

**Plataforma:** windows · **Estado:** feito · **Esforço:** M

## O que é

Disparar capturas de fora, por um `rustshot://` registrado no sistema.

## Como fazer

Registrar o esquema no registro e mapear os parâmetros para os modos que a CLI já aceita.

## Como ficou

Entregue em 28/08/2026: uma chave nas Configurações registra `rustshot://`, e a partir daí um link dispara o que a linha de comando já fazia.

```
rustshot://fullscreen[?copy=1][&save=1]
rustshot://open?file=<caminho>
rustshot://clipboard
rustshot://ocr?file=<caminho>
```

**A URL vira o mesmo `Mode` da linha de comando**, e não um caminho paralelo: o que o link pode pedir é exatamente o que a CLI já faz, com as mesmas regras — inclusive a de que `fullscreen` sem pedido explícito salva.

**Só entram os comandos que rodam sem janela e sem residente.** Região e edição dependem de um overlay sobre a tela congelada, que é trabalho do residente; um processo disparado por um link não tem como assumir isso, e prometer `rustshot://region` seria prometer um comportamento que depende de o programa já estar aberto.

**O registro fica em `HKCU\Software\Classes`**, no ramo do usuário e sem elevação — o mesmo princípio do resto do estado, que mora ao lado do executável. O esquema aponta para o caminho atual do `.exe`; mover a pasta o quebra até alguém registrar de novo, que é o preço já pago por "Iniciar com o Windows".

**A caixa das Configurações age fora do rascunho**, e é a única que faz isso: o registro é um efeito no sistema, não uma preferência do `config.json` — quem copiar o arquivo para outra máquina não leva o registro junto. Por isso ela lê o registro ao desenhar e escreve nele no clique, sem esperar o botão Salvar. Um esquema apontando para **outro** executável conta como não registrado, para o usuário ver a caixa desmarcada e poder consertar clicando.

**A decodificação de `%XX` é feita aqui**, porque o shell entrega a URL como o autor a escreveu e um caminho com espaço chega percent-encoded.

Um comando desconhecido é recusado com erro de uso, em vez de cair no padrão: capturar a tela por engano é pior que não fazer nada.
