# Salvar e copiar automáticos

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

Depois de capturar, salvar e/ou copiar sem passar pelo editor.

## Como fazer

As ações já existem no fluxo de tela cheia; é expor a escolha nas Configurações.

## Como ficou

Entregue em 28/08/2026: dois seletores na seção "Capturas" das Configurações — o que fazer depois da tela cheia e o que fazer depois de uma região —, cada um com salvar, copiar ou os dois.

**Uma escolha por fluxo, e não uma só.** Os dois nasceram com padrões diferentes de propósito: quem aciona a tela cheia costuma querer o arquivo, e quem recorta uma região costuma querer colar em seguida. Um seletor único teria de mudar o comportamento de um dos dois só para existir; com dois, os padrões continuam sendo exatamente o que eram e ninguém é surpreendido ao atualizar.

Vale também para **repetir a última região**, que é uma captura de região sem o overlay.

Quando os dois estão ligados, **a cópia vai primeiro**: ela é imediata e é o que o usuário está esperando para colar, enquanto o arquivo ainda tem de ser codificado.

A linha de comando ficou como estava. `rustshot --capture-fullscreen` sem `--copy` nem `--save` continua salvando, como o `--help` sempre prometeu: um script que espera um arquivo não pode passar a só copiar porque alguém mexeu numa janela de configurações.
