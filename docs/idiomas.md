# Idiomas

Atualizado: 2026-11-05

O produto fala inglês e português do Brasil, decidido em 2026-11-05. O inglês é a base: qualquer língua que não seja português aparece em inglês. Qualquer variante de português (`pt`, `pt-BR`, `pt-PT`) recebe o texto em português do Brasil. Não há escolha de idioma dentro do app.

## Onde cada frontend decide

| Frontend | Segue | Onde estão os textos |
| --- | --- | --- |
| Bandeja do desktop | Língua do sistema (`sys-locale`); `SATCHEL_LANG=pt` ou `en` força | `crates/satchel-desktop/src/text.rs` e `app.rs` (`tray_view`) |
| Página de setup | `Accept-Language` do navegador; sem português nem inglês, a língua da bandeja | `crates/satchel-desktop/src/setup_ui.rs`, `text.rs` (`describe`) |
| Android | Língua do sistema ou a língua por app do Android | `res/values/strings.xml` (inglês) e `res/values-pt/strings.xml` |
| iOS | Língua do sistema | `ios/Satchel/Localizable.xcstrings` e `InfoPlist.xcstrings`, chave em inglês, tradução `pt-BR` |

Os erros da página de setup e da bandeja são frases com o próximo passo, nas duas línguas. O texto técnico do erro vai só para o log. Na bandeja, uma sincronização que falhou diz o que conferir: chaveiro, armazenamento, pasta ou o log.

## Termos

O mesmo termo nos três frontends:

| Português | Inglês |
| --- | --- |
| catálogo | catalog |
| aparelho / Este aparelho | device / This device |
| Ver · Baixar agora · Sincronizar sempre | View · Download now · Always sync |
| Sincronizado · Último envio · Registrado | Synced · Last sent · Registered |
| senha de recuperação | recovery password |
| convite · código · impressão digital · endereço · apelido | invite · code · fingerprint · address · nickname |
| pasta · armazenamento · chave | folder · storage · key |
| Tentar de novo · Entrar · Aprovar · Recusar · Criar · Copiar | Try again · Join · Approve · Deny · Create · Copy |
| Adicionar aparelho | Add device |

Estilo do inglês: frases curtas, só a primeira letra maiúscula, sem ponto de exclamação, espera no gerúndio (“Reading the catalog…”).

## Data e hora

- Português: `dd/MM/yyyy HH:mm` e o nome curto do fuso.
- Inglês: `yyyy-MM-dd HH:mm` e o nome curto do fuso, para dia e mês não se confundirem.
- Bandeja: só `HH:mm` no fuso local quando é o mesmo dia; antes disso, `yyyy-MM-dd HH:mm` nas duas línguas.

## Ainda numa língua só

- O aparelho de exemplo do catálogo de amostra se chama “Publicador”, sistema “exemplo” (`crates/satchel-ffi/src/lib.rs`). É dado de demonstração, não texto de tela.
- O apelido padrão do celular continua “iPhone”, também no Android.
- O log do desktop (stderr) fica em inglês.
