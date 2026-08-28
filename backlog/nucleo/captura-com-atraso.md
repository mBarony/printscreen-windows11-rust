# Captura com atraso

**Plataforma:** nucleo · **Estado:** feito · **Esforço:** P

## O que é

Conta alguns segundos antes de congelar a tela.

## Como fazer

Timer antes de disparar a captura. O valor entra no `config.json`.

## Como ficou

Entregue em 28/08/2026: item "Capturar tela cheia em 3 s" no menu da bandeja.

A espera roda em thread de trabalho e a captura volta para a fila de eventos
em vez de acontecer lá. Dois motivos: capturar de fora da thread da bandeja
mexeria em GDI de outro contexto, e bloquear a thread de mensagens
congelaria o ícone durante os três segundos.

Um balão avisa que a contagem começou — sem ele, três segundos de silêncio
depois de clicar num menu parecem um clique que não pegou.
