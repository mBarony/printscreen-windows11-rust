# Toda tecla de atalho configurada pressionando

**Plataforma:** nucleo · **Estado:** falta · **Esforço:** P

## O que é

Em Configurações, qualquer atalho é definido pressionando a tecla desejada, em
vez de escolhida numa lista — e o alfabeto aceito inclui numerais.

## Onde já está pronto

Os **quatro atalhos globais** (tela cheia, região, região + edição, reconhecer)
já têm o modo "pressione a combinação" (`settings.rs:394` e `:468`, alimentado
por `capture_combo`, `settings.rs:580`), e a lista `KEY_CHOICES`
(`settings.rs:46`) já traz `Digit0`–`Digit9`, F1–F12, `Space`, `Enter` e
pontuação. Essa metade do pedido já está atendida.

## O que falta

As **16 teclas de ferramenta do editor**. Hoje cada uma é um `ComboBox`
alimentado por `for c in 'A'..='Z'` (`settings.rs:285`): sem captura por
pressionamento e sem numerais.

## Como fazer

Trocar o combo por um botão que entra no mesmo modo de captura dos atalhos
globais, reaproveitando `capture_combo` — filtrando os modificadores, porque a
tecla de ferramenta é sempre seca.

`ToolKeysConfig` guarda `String`, então o formato não muda; o que muda é o
conjunto de valores possíveis. Para aceitar numerais, o casamento no editor
precisa ser por `Code` (`"Digit5"`) e não por letra: hoje ele compara caractere.
Alinhar com o nome que o resto da configuração já usa evita um segundo
vocabulário de teclas dentro do mesmo `config.json`.

## Notas

`tool_key_conflicts` (`settings.rs:537`) já avisa quando duas ferramentas
disputam a mesma tecla, e os testes cobrem o caso do recuo para o padrão.
Ampliar o alfabeto amplia o espaço de conflito para fora dessa checagem: uma
tecla de ferramenta pode passar a colidir com atalho do próprio editor — a
janela já usa `Ctrl+…` para quase tudo, mas `Delete`, `Enter` e `Espaço` têm
significado seco lá dentro. O aviso precisa cobrir esses, ou a captura precisa
recusá-los na origem.
