# Captura de tela

**Plataforma:** linux · **Estado:** falta · **Esforço:** G

## O que é

Congelar o conteúdo dos monitores, equivalente ao `BitBlt` do Windows.

## Como fazer

`ext_image_copy_capture_manager_v1` + `ext_output_image_capture_source_manager_v1`
+ `wl_shm`, falando `wayland-client` direto — que é o que o omasnap faz em
`src/surface-capture.cpp`.

**Sem portal e sem diálogo de permissão**, ao contrário do que acontece em
GNOME/KDE. A geometria dos monitores vem de `hyprctl monitors -j`.

## Notas

Alvo definido: **Hyprland**, compositor Wayland baseado em wlroots. A escolha
importa — o que segue não vale para GNOME nem KDE, onde a captura passa pelo
portal e a enumeração de janelas não é exposta.

O omasnap, que originou este projeto, roda exatamente nesse ambiente e serve
de implementação de referência.
