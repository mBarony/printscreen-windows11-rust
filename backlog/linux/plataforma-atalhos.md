# Atalhos globais

**Plataforma:** linux · **Estado:** falta · **Esforço:** P

## O que é

Disparar as capturas por tecla, sem o app estar em foco.

## Como fazer

Não se implementa: **quem faz esse papel é o compositor**. O usuário
acrescenta ao `hyprland.conf`:

```
bind = , Print, exec, rustshot --capture-region
```

O binário é invocado, faz o trabalho e sai.

## Notas

Isto **simplifica a arquitetura**: no Linux não há processo residente, nem
bandeja, nem IPC entre processos — três dos módulos mais complexos do lado
Windows simplesmente não existem aqui.
