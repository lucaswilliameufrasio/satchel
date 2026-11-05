# Identidade visual

Atualizado: 2026-11-05

Proposta para o item “Telas com cara de produto”. Ela parte da superfície que o iOS já tinha: papel quente, tinta escura, couro e a marca da bolsa sobre verde-escuro. O modo escuro e as regras de estado são novos. Nada aqui muda regra de segurança.

## Cores

Os nomes são os mesmos nos três frontends: `SatchelInk` no iOS, `SatchelColors` no Android e variáveis CSS na página de setup.

| Papel | Claro | Escuro | Uso |
| --- | --- | --- | --- |
| canvas | `#F3EEE4` | `#17130F` | Fundo da tela |
| card | `#FFFCF7` | `#221C17` | Cartões e painéis |
| ink | `#241C16` | `#F2ECE3` | Texto principal |
| secondary | `#6B5E52` | `#B8AB9C` | Texto de apoio e rótulos |
| accent | `#8C4A24` | `#E0A070` | Ação, link e estado escolhido |
| field | `#EBE4D8` | `#2D261F` | Fundo de campo de texto |
| line | ink a 14% | ink a 14% | Borda de cartão |
| danger | `#A3261B` | `#F08A7E` | Erro e ação que apaga |
| selected | accent a 14% | accent a 18% | Fundo da opção escolhida |

Contraste WCAG, todos AA para texto comum (≥ 4,5:1): `secondary` sobre `field` é o menor par, 4,96 no claro e 6,63 no escuro. O botão principal usa texto `card` sobre `accent`: 6,58 no claro e 7,57 no escuro.

## Tipo

A fonte é a do sistema (SF no iOS e no macOS, Roboto no Android, `system-ui` na página). Os tamanhos seguem o tamanho de fonte que a pessoa escolheu no sistema.

- Título da tela: 32, seminegrito.
- Título de cartão: headline (17), seminegrito.
- Corpo: body (17 no iOS, 16 no Android e na página).
- Apoio: subheadline (15), cor `secondary`.
- Rótulo de campo: footnote (13), seminegrito, `secondary`.
- Id de aparelho, senha de recuperação e código de pareamento: monoespaçada.

## Forma e espaço

- Cartão: raio 18, borda `line` de 1, preenchimento 16.
- Campo: raio 12, preenchimento 12, fundo `field`, sem borda.
- Botão principal: largura toda, raio 14, altura mínima 48, fundo `accent`, texto `card`. Um por tela.
- Botão secundário: texto `accent`, sem fundo.
- Margem lateral 20 e espaço entre cartões 12. A página de setup limita a largura a 34rem.
- Área de toque mínima de 44 pt no iOS e 48 dp no Android.

## Marca

A bolsa de couro sobre verde-escuro (`SatchelMark` no iOS, mipmap no Android). Aparece no topo da tela inicial, com “Satchel” ao lado, e não se repete nas telas internas.

## Estados

Toda tela que espera, falha ou está vazia diz o que está acontecendo e o próximo passo, numa frase cada:

- Espera: o verbo no gerúndio (“Lendo o catálogo…”) e a ação desligada.
- Vazio: o que falta e como resolver (“Nenhum arquivo ainda. Ponha arquivos na pasta do desktop ou escolha uma pasta neste aparelho.”).
- Erro: o que não deu certo, sem código técnico, e o que fazer (“Confira a rede e tente de novo.”). O botão de tentar de novo fica na mesma tela.
- Feito: uma linha curta, sem ponto de exclamação.

## Acessibilidade

- Modo escuro segue o sistema.
- Tamanho de fonte do sistema: nenhum texto com tamanho fixo, a não ser o título da tela, que escala no iOS com `relativeTo: .largeTitle`.
- Leitor de tela: a linha inteira do catálogo é um botão com rótulo que junta nome, tamanho e estado. Ícones decorativos ficam fora da leitura. Títulos de tela são cabeçalhos.
- Idioma: inglês e português do Brasil, com os termos em [idiomas.md](idiomas.md).
