# Upload para S3

**Plataforma:** windows · **Estado:** falta · **Esforço:** G

## O que é

Envia a captura para armazenamento compatível com S3 e copia o link.

## Como fazer

Precisa de HTTP com TLS e assinatura AWS SigV4. O caminho que preserva a
ausência de crates de rede é `WinHttp`, que é Win32 e já está no sistema; a
assinatura seria código próprio.

## Notas

Rever a promessa de privacidade do README antes: um app que faz upload não é mais "sem rede", mesmo que só quando pedido.
