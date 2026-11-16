# Identidade visual

Atualizado: 2026-11-16

Direção C de [referencias-visuais.md](referencias-visuais.md), escolhida em 2026-11-16 por quem decide o produto: superfície, espaço e tom do Mercury; status e dados do Geist (Vercel). Substitui a proposta de 2026-11-05 (papel quente e couro), que não tinha sido aprovada. Nada aqui muda regra de segurança.

## Cores

Os nomes são os mesmos nos três frontends: `SatchelInk` no iOS, `SatchelColors` no Android e variáveis CSS na página de setup. `success` e `warning` existem por enquanto só na página.

| Papel | Claro | Escuro | Uso |
| --- | --- | --- | --- |
| canvas | `#FBFCFD` | `#10101A` | Fundo da tela, neutro levemente azulado |
| card | `#FFFFFF` | `#1C1C28` | Cartões e painéis |
| ink | `#272735` | `#EDEDF3` | Texto principal |
| secondary | `#666673` | `#A3A3AE` | Texto de apoio e rótulos |
| accent | `#4B5FE0` | `#9CB4E8` | Ação, link e estado escolhido |
| field | `#F0F1F5` | `#2A2A38` | Fundo de campo de texto |
| line | `#DDDDE5` | `#363644` | Borda de cartão |
| danger | `#C02868` | `#FC92B4` | Erro e ação que apaga |
| success | `#187A4E` | `#77C599` | Sincronizou |
| warning | `#B34900` | `#FC9B6F` | Aviso que não é erro (sem versionamento, chave compartilhada) |
| selected | accent a 12% | accent a 18% | Fundo da opção escolhida |

Os valores partem dos tokens do Mercury, escurecidos no claro até passar AA. Contraste WCAG medido em 2026-11-16, todos ≥ 4,5:1 para texto comum: o menor par é `accent` sobre `field` no claro, 4,64. O botão principal usa texto `card` sobre `accent`: 5,23 no claro e 8,11 no escuro.

## Tipo

A fonte é a do sistema (SF no iOS e no macOS, Roboto no Android, `system-ui` na página). Os tamanhos seguem o tamanho de fonte que a pessoa escolheu no sistema.

- Título da tela: 32, seminegrito.
- Título de cartão: headline (17), seminegrito.
- Corpo: body (17 no iOS, 16 no Android e na página).
- Apoio: subheadline (15), cor `secondary`.
- Rótulo de campo: footnote (13), seminegrito, `secondary`.
- Id de aparelho, senha de recuperação e código de pareamento: monoespaçada.

## Forma e espaço

- Cartão: raio 12, borda `line` de 1, preenchimento 16.
- Campo: raio 8, preenchimento 12, fundo `field`, sem borda.
- Botão principal: largura toda, raio 10, altura mínima 48, fundo `accent`, texto `card`. Um por tela.
- Botão secundário: texto `accent`, sem fundo.
- Margem lateral 20 e espaço entre cartões 12. A página de setup limita a largura a 34rem.
- Área de toque mínima de 44 pt no iOS e 48 dp no Android.

## Marca

A bolsa de couro sobre verde-escuro (`SatchelMark` no iOS, mipmap no Android). Aparece no topo da tela inicial, com “Satchel” ao lado, e não se repete nas telas internas.

## Estado da sincronização

Como o Status Dot do Geist: um ponto antes da frase de estado, verde (`success`) quando sincronizou, vermelho (`danger`) quando falhou e neutro (`secondary`) antes da primeira rodada. A frase diz o mesmo, com a hora (“Sincronizado às 14:02”); a cor nunca é o único sinal. Números, horas, tamanhos e códigos usam algarismos tabulares. Hoje o ponto está na página do desktop; nos celulares ainda não.

## Estados

Toda tela que espera, falha ou está vazia diz o que está acontecendo e o próximo passo, numa frase cada:

- Espera: o verbo no gerúndio (“Lendo o catálogo…”) e a ação desligada.
- Vazio: o que falta e como resolver (“Nenhum arquivo ainda. Ponha arquivos na pasta do desktop ou escolha uma pasta neste aparelho.”).
- Erro: o que não deu certo, sem código técnico, e o que fazer (“Confira a rede e tente de novo.”). O botão de tentar de novo fica na mesma tela.
- Feito: uma linha curta, sem ponto de exclamação.

## Acessibilidade

- Modo escuro segue o sistema.
- Tamanho de fonte do sistema: nenhum texto com tamanho fixo, a não ser o título da tela, que escala no iOS com `relativeTo: .largeTitle`. Nos tamanhos de acessibilidade a seta dos cartões some, e a marca de opção escolhida fica na linha do título, para o texto manter a largura.
- Voltar: o gesto de voltar do sistema faz o mesmo que o Voltar da tela; enquanto a tela espera, não faz nada. A tela da senha de recuperação não volta.
- Leitor de tela: a linha inteira do catálogo é um botão com rótulo que junta nome, tamanho e estado. Ícones decorativos ficam fora da leitura. Títulos de tela são cabeçalhos.
- Idioma: inglês e português do Brasil, com os termos em [idiomas.md](idiomas.md).
