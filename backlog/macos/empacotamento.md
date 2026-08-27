# Empacotamento, assinatura e notarização

**Plataforma:** macos · **Estado:** falta · **Esforço:** G

## O que é

Entregar o app de forma que abra em máquina alheia.

## Como fazer

`.app` bundle, `.dmg`, assinatura com Developer ID e **notarização** pela
Apple. Sem notarização o Gatekeeper bloqueia o app em qualquer máquina que
não seja a de quem compilou.

Exige conta de desenvolvedor Apple paga (99 USD/ano) e um runner macOS no CI.

## Notas

O obstáculo do macOS é de distribuição, não de código.
