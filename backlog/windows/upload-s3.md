# Upload para S3

**Plataforma:** windows · **Estado:** cancelado · **Esforço:** G

## O que é

Envia a captura para armazenamento compatível com S3 e copia o link.

## Como fazer

Precisa de HTTP com TLS e assinatura AWS SigV4. O caminho que preserva a
ausência de crates de rede é `WinHttp`, que é Win32 e já está no sistema; a
assinatura seria código próprio.

## Notas

Rever a promessa de privacidade do README antes: um app que faz upload não é mais "sem rede", mesmo que só quando pedido.

## Por que não

Cancelado em 28/08/2026, por decisão de produto.

O RustShot não fala com a rede. Isso não é um detalhe de implementação que sobrou: é uma propriedade **verificável** — a árvore de dependências não tem cliente HTTP, e o binário não abre soquete nenhum. Quem precisa confiar uma captura de tela a um programa pode conferir isso sozinho, e essa conferência vale mais do que a comodidade de subir um arquivo.

O upload quebraria a propriedade mesmo saindo por `WinHttp`, e mesmo só quando pedido. A partir do momento em que o código existe, a resposta a "este programa manda minha tela para algum lugar?" deixa de ser "não" e passa a ser "só se você mandar" — que é uma resposta pior, porque exige auditar o comportamento em vez de auditar a lista de dependências.

Quem quiser subir uma captura tem caminhos que não custam essa propriedade: a pasta de destino é configurável e pode apontar para uma pasta sincronizada, e o `rustshot://` (ou a linha de comando) deixa qualquer script fazer o envio depois de salvar.

**Se um dia for reaberto**, o que muda primeiro é a promessa, não o código: o README precisa dizer com todas as letras o que passa a ser possível, antes de a primeira linha ser escrita.
