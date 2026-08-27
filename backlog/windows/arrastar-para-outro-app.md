# Arrastar para outro aplicativo

**Plataforma:** windows · **Estado:** falta · **Esforço:** M

## O que é

Tirar a captura do editor arrastando direto para outro programa.

## Como fazer

Implementar `IDropSource` e `IDataObject` (COM) e chamar `DoDragDrop`. É a
única parte que precisaria de vtables COM à mão sobre o `windows-sys` — ou
usar a crate `windows`, que já está no binário por causa do OCR.
