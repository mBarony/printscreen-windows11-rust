# Empacotamento e release

**Plataforma:** linux · **Estado:** falta · **Esforço:** M

## O que é

Entregar o binário de forma instalável.

## Como fazer

`PKGBUILD` para o AUR é o caminho natural do público de Hyprland (Arch), que
é o mesmo público do omasnap. Um binário estático e um `.desktop` cobrem o
resto.

O CI ganha um job Linux; o release passa a publicar dois artefatos.
