# Catálogo no telefone

Atualizado: 2026-11-02

A apresentação do catálogo no iOS e no Android. O Android deste checkout segue este texto na lista, na tela do arquivo, na entrada e no mipmap. O iOS em `ios/Satchel` mostra o mesmo texto numa superfície própria: cartões numa rolagem, com a marca sem texto no topo.

Isto não muda o JSON do catálogo, os estados `see`, `once` e `always`, nem o que cada estado faz. A primeira abertura continua listando metadados e não baixa o arquivo.

## Por que a lista não mostra o hex

O catálogo conhece o id opaco e, quando o aparelho publicou, o apelido, o sistema e as datas. Colocar os 32 hexadecimais na linha fazia o id ser o conteúdo do cartão.

A lista diz quantos aparelhos mantêm cópia e não mostra o hex. A tela do arquivo mostra cada aparelho com os dados que o snapshot tiver, e o id inteiro em grupos de quatro, sem cortar e sem inventar nome. O id deste aparelho leva o título Este aparelho, e o id agrupado fica na linha de baixo. Outro aparelho usa o apelido publicado. Sem apelido, o título é o id. Dois ids podem compartilhar um prefixo, então a lista não abrevia e nenhum id vira um prefixo curto.

No exemplo, sem concessão, este aparelho é o leitor `fedcba9876543210fedcba9876543210`. Com concessão, o título compara com o arquivo `device-id` que o pareamento já gravou. Essa leitura não cria o arquivo. Se a concessão existe e o arquivo não está lá, nenhum id ganha o título.

O apelido e o sistema são os que o aparelho publicou no snapshot cifrado. A data de registro é o relógio do primeiro snapshot que levou esse apelido. O último envio é o relógio do snapshot mais novo daquele id. Não há um registro separado de “abriu o aplicativo”. `keys/` continua sem nome e sem `created`. Se o snapshot não tem o campo, a linha não aparece.

A data é o `versionUnix` convertido para o fuso do aparelho. O formato é `dd/MM/yyyy HH:mm` mais o nome curto desse fuso. Um aparelho em UTC−3 mostra o exemplo `1700000000` como `14/11/2023 19:13` e o nome curto do fuso, não `22:13 UTC`. O tamanho entra na lista porque Ver, no plano, inclui o tamanho, e o campo `size` já vinha no JSON sem aparecer.

## Lista

Título grande: Catálogo.

Se ainda não há concessão, a primeira linha é um botão:

- Título: Entrar neste aparelho
- Apoio: Ler o QR mostrado no desktop, ou informar o convite.

O toque abre a entrada. Não dispara o pareamento.

Sem arquivos: Nenhum arquivo ainda. O que este ou outro aparelho enviar aparece aqui.

Cada arquivo ou pasta é uma linha. O toque abre a tela daquele caminho. A linha inteira é o botão. O estado à direita não é um segundo botão.

- Ícone: pasta se `directory` for verdadeiro, documento se for arquivo. O ícone não substitui o texto.
- Título: `path`, sem reformatar.
- Apoio, arquivo: `<tamanho> · <data>`. A data usa o fuso do aparelho. Em UTC−3 o exemplo fica `13 B · 14/11/2023 19:13` mais o nome curto do fuso.
- Apoio, pasta: `Pasta · <data>`. Pasta não mostra `0 B`.
- Nota: `Nenhum aparelho`, `1 aparelho` ou `N aparelhos`.
- Trailing, cor secundária: Ver, Baixar agora ou Sincronizar sempre.

Tamanho, base 1024:

| Bytes | Texto |
| --- | --- |
| 0 a 1023 | `13 B` |
| a partir de 1 KiB, valor inteiro ou ≥ 10 | `2 KiB`, `12 MiB` |
| a partir de 1 KiB, fração menor que 10 | `1.5 KiB` |

Unidades, nesta ordem: B, KiB, MiB, GiB, TiB. O separador decimal é ponto. Tamanho negativo não ocorre no JSON; se aparecer, mostrar `0 B`.

## Arquivo

Título da navegação: o `path`. Voltar chama Catálogo e fica desligado enquanto a escolha está sendo gravada.

Primeira seção, sem título:

- Tamanho: o mesmo texto da lista, ou Pasta.
- Sincronizado: a mesma data, no fuso do aparelho.

Seção Neste aparelho. Três linhas, nesta ordem. O selecionado leva um checkmark e o trait de selecionado. Durante a gravação as três ficam desligadas e a seção mostra Salvando a escolha….

| Rótulo | Texto de apoio |
| --- | --- |
| Ver | O conteúdo fica no storage. Este aparelho não baixa. |
| Baixar agora | Traz o arquivo uma vez e não acompanha mudanças posteriores. |
| Sincronizar sempre | Baixa, envia e acompanha. |

Pasta com Sincronizar sempre acrescenta: Arquivo novo dentro desta pasta entra nesse estado.

Seção Aparelhos. O id deste aparelho tem o título Este aparelho. Outro aparelho usa o apelido publicado, se houver. Sem apelido, o título é o id agrupado. Embaixo, só quando o snapshot trouxer o dado:

- Apelido, quando o título ainda não é esse nome.
- Sistema.
- Registrado, no mesmo formato de data do arquivo.
- Último envio, no mesmo formato.

O id agrupado fica na última linha, monoespaçado, quando o título não é o próprio id. Lista vazia: Nenhum aparelho mantém cópia. Um aparelho sem apelido ainda pode gravar Ver, Baixar agora ou Sincronizar sempre. Esse cartão não ganha Apelido, Sistema nem Registrado. O exemplo novo traz o publicador como Publicador, sistema exemplo, registrado e último envio em `1700000000`. Depois de Baixar agora, este aparelho entra só com o último envio. Um catálogo de exemplo já gravado no cache não tem esses campos até a pasta `sample` ser recriada.

Escolher Ver, Baixar agora ou Sincronizar sempre continua na tela do arquivo e relê o catálogo. Ver não baixa. Baixar agora e Sincronizar sempre leem o conteúdo quando o estado pede, como o núcleo já faz.

## Entrada

Título: Entrar neste aparelho. Voltar volta ao catálogo.

Texto acima dos campos: O desktop mostra o QR. A senha de recuperação não faz parte do convite.

Campos, nesta ordem: Endereço, Código, Impressão digital, Apelido. Endereço e impressão digital sem correção automática nem maiúsculas. O apelido começa em iPhone, ou no apelido já gravado. Ele tem de 1 a 64 caracteres e não aceita caractere de controle. Entrar grava esse apelido neste aparelho e o manda na aprovação. O próximo snapshot deste aparelho publica apelido, sistema iOS e a data de registro.

Falha: Não foi possível entrar. Confira o código e se o outro aparelho ainda mostra o convite. O texto do QR rejeitado não aparece.

Ações: Ler QR e Entrar. Ler o QR preenche os campos e volta para esta tela. A pessoa ainda toca Entrar. Nada entra sozinho depois do scan.

Câmera, título Ler QR: Aponte a câmera para o QR do desktop.

## Estados vazios

Carregando, centralizado: Lendo o catálogo…

Falha ao abrir, centralizado: Não foi possível abrir o catálogo. Confira a rede e tente de novo. Abaixo, o botão Tentar de novo.

## Logo

`assets/satchel-icon.png` é a marca, 1024×1024, sem texto. O iOS usa essa imagem como ícone único do app e, no topo da superfície do catálogo, como o recurso `SatchelMark`. A bandeja do macOS usa `assets/satchel-tray.rgba` como template do item de status. Os rótulos dela mudaram em 2026-11-05 e estão em inglês e português (ver [idiomas.md](idiomas.md) e a nota “bandeja e idiomas” em [progresso.md](progresso.md)). A vista enviada traz a pasta ou “No folder set up”, Sync now, Add device…, Show devices, as linhas de aprovação (quem quer entrar, sistema, endereço e interface, código digitado, Approve, Deny), as linhas de adicionar aparelho (código para digitar, QR, endereço, impressão digital em grupos, Stop adding), Remove device com o id opaco do outro aparelho, e Quit Satchel. Remove device não oferece a chave de recuperação nem a chave deste desktop. A senha de recuperação não aparece. O Linux continua com `folder-remote`.

## Android

O Compose em `android/.../catalog/Catalog.kt` usa este texto. A linha da lista não mostra o hex. Na tela do arquivo, o id local leva o título Este aparelho e o hex agrupado fica embaixo. Cada aparelho mostra apelido, sistema, Registrado e Último envio só quando o JSON `holders` trouxer `name`, `os`, `enrolledUnix` e `lastUnix`. Campo ausente não vira texto. No exemplo, esse id é o leitor `fedcba9876543210fedcba9876543210`, e o publicador vem como Publicador. Com concessão, a comparação usa o `device-id` já gravado, sem criar o arquivo. O apelido digitado na entrada é gravado e enviado no mark, com sistema Android. Mark sem apelido continua válido: o JSON desse aparelho traz o id e `lastUnix`, sem `name`. Os mipmaps `ic_launcher` são escalas de `assets/satchel-icon.png`, sem texto acrescentado. `android:allowBackup` continua `false`.

Em `Catalog.kt`, o apoio da linha é `catalogMeta`. `rowWhen` formata `versionUnix` no fuso do aparelho, `dd/MM/yyyy HH:mm` mais o nome curto do fuso. O teste que espera `14/11/2023 22:13 UTC` usa um `ZoneId` UTC explícito. O instante `1700000000` num `ZoneOffset` de UTC−3 aparece como `14/11/2023 19:13` mais o nome curto desse deslocamento. O id agrupado e a contagem de aparelhos estão na lista. `followLabel` permanece.

Este Mac não sobe emulador Android. Em 2026-11-02 a prova rodou no host `ursoc-tailscale`, AVD `satchel_api35`, sem janela, fuso `America/Belem`. A primeira abertura, duas vezes, a partir de um exemplo novo, mostrou `kept.txt`, `13 B · 14/11/2023 19:13 America/Belem`, `1 aparelho` e Ver. A linha não tinha `2023-11-14T22:13:20Z` nem o id de 32 hex. A pasta `work` não tinha `kept.txt`. O nome curto que o Android devolveu para esse fuso foi o próprio id `America/Belem`. Baixar agora, duas vezes a partir de um exemplo novo, gravou `payload-bytes` uma vez e a leitura seguinte ficou em Baixar agora, com `2 aparelhos`. O emulador foi desligado depois. Não houve pareamento, câmera nem segundo aparelho.

## Prova no simulador

No iPhone 17, a lista mostrou `kept.txt`, o tamanho, a data e a contagem de aparelhos, com o estado na mesma linha do nome. Com o fuso do aparelho, o mesmo instante apareceu como `14/11/2023 19:13 GMT-3`. A data não quebrou quando o estado passou a Baixar agora. A tela do arquivo mostrou tamanho, data, as três frases e os ids em grupos de quatro. O teste tocou Baixar agora: o checkmark foi para essa linha e a lista passou a dizer Baixar agora e 2 aparelhos. A câmera não abriu.

Na passagem seguinte, a tela de `kept.txt` mostrou o outro id agrupado e, embaixo, Este aparelho com `fedc ba98 7654 3210 fedc ba98 7654 3210`. O teste de interface achou esse título. A tela inicial do simulador mostrou o ícone da bolsa com o nome Satchel. A mesma marca apareceu na tela inicial do iPhone de Lucas. Abrir o app nesse aparelho pediu verificação do desenvolvedor na rede e o catálogo não abriu. A bandeja do macOS não foi iniciada, porque esse processo usa o serviço de cofre `satchel`. A silhueta de 36×36 foi conferida à parte, e o teste `menu_bar_mark_is_a_36_pixel_template` passou.

Na passagem dos cartões, antes do toque, `kept.txt` mostrou Publicador, Sistema exemplo, Registrado `14/11/2023 19:13 GMT-3`, Último envio na mesma data e o id agrupado do publicador. O primeiro toque em Baixar agora caiu em “Não foi possível abrir o catálogo.”, porque a ABI em C recusava apelido vazio. O mark passou a aceitar apelido e sistema vazios. No host, `empty_device_label_still_marks_the_sample` passou. No simulador, o toque seguinte marcou Baixar agora, manteve o cartão do Publicador e mostrou Este aparelho só com Último envio `14/11/2023 19:13 GMT-3` e o id do leitor. A lista voltou com Baixar agora e 2 aparelhos. A câmera não abriu. A build iphoneos foi assinada, `codesign --verify --deep --strict` passou, e `devicectl device install app` instalou de novo em `com.lucaswilliameufrasio.satchel.catalogcheck`. A tela do iPhone estava bloqueada, então esses cartões não foram vistos no aparelho.

## Superfície de produto

Em 2026-11-02 o catálogo, o arquivo e a entrada do iOS passaram a uma rolagem de cartões. A superfície enviada identifica-se como `satchel-product-surface`, com valor `scroll-stack`. A marca sem texto, a mesma de `assets/satchel-icon.png`, aparece no topo como `SatchelMark`, além do ícone da tela inicial. O texto continua o desta página.

No simulador iPhone 17 (iOS 27.0, `C66965B9-2ACB-4AE4-BB44-FB3A4489CC40`), duas aberturas a partir de uma instalação limpa mostraram a marca, a palavra Satchel, o título Catálogo e cartões. A linha foi `kept.txt`, Ver, `13 B · 14/11/2023 19:13 GMT-3` e `1 aparelho`. Não havia `2023-11-14T22:13:20Z` nem o id de 32 hex. Em cada passagem, Baixar agora ficou selecionado e a lista voltou com Baixar agora e `2 aparelhos`. O exemplo na tela era só `kept.txt`. A câmera não abriu. O iPhone de Lucas não foi aberto.

A suíte da superfície, no mesmo simulador, leu a vista enviada. Viu a marca, os cartões, Este aparelho com Último envio `14/11/2023 19:13 GMT-3` e o id agrupado `fedc ba98 7654 3210 fedc ba98 7654 3210`, os campos Endereço, Código, Impressão digital e Apelido, o instante `1700000000` em UTC−3 como `14/11/2023 19:13 GMT-3` e em UTC explícito como `14/11/2023 22:13 UTC`, o QR preenchendo sem enviar, e o arquivo de chave sem o apelido. Foram 4 testes, 0 falhas. A sessão imprimiu `** TEST SUCCEEDED **`.

A bandeja do macOS usa `assets/satchel-tray.rgba` como template do item de status. A vista enviada traz a pasta ou No folder, Sync, Add device, Devices, as linhas de aprovação (Device, System, Address, Interface, Code, Approve, Deny), as linhas de adicionar aparelho (Code, Address, Fingerprint, Cancel), o PNG do convite enquanto adicionar está aberto, Revoke com o id opaco do outro aparelho, e Quit. Revoke não oferece a chave de recuperação nem a chave deste desktop. A senha de recuperação não aparece. O Linux continua com `folder-remote`. Os rótulos em inglês não foram traduzidos.

Nesta máquina, `cargo fmt --all --check` e `cargo clippy --all-targets -- -D warnings` saíram 0. O `rustic_core` ainda avisa que o lint `no_mangle_generic_items` foi removido. Duas vezes cada, um teste por filtro: `tray_view_is_the_product_surface`, `add_device_screen_approves_without_the_recovery_password`, `publish_lists_metadata_and_revoke_removes_only_the_device_key` e `tray_binary_follows_the_connected_display`. O teste headless passou em 1,32 s nas duas vezes, antes de o display ser acordado. Sozinho, ele também aceita a recusa quando não há display. O display estava adormecido: `CGGetActiveDisplayList` voltou com contagem 0. Depois de `caffeinate -u`, a contagem foi 2. As duas aberturas seguintes de `satchel-desktop`, com `XDG_CONFIG_HOME` temporário e serviço de cofre `satchel-test-…`, ficaram de pé e foram encerradas. O stderr ficou vazio e a busca por recovery saiu 1. O System Events viu o processo e devolveu o erro -1719 ao pedir a menu bar 1, então os títulos não foram lidos. O `cargo test` completo e o Floci não foram repetidos neste corte.

## Criar e mostrar o convite

O catálogo oferece Criar repositório enquanto o aparelho não tem concessão, e Mostrar convite no aparelho que criou. A tela de criar escolhe Pasta, S3, R2 ou B2. Em objeto pede endpoint, bucket, região, prefixo, a chave de manutenção e a chave dos aparelhos, que precisa ser outra. Depois de criar vem a tela Senha de recuperação, com o texto “Guarde esta senha num gerenciador de senhas que abre sem este repositório. Ela aparece só agora e o restic oficial usa esta senha.” e o botão Guardei. O convite mostra Código, Endereço, Impressão digital e o QR, e diz que a senha de recuperação não vai no convite. Quando o outro aparelho passa pela checagem do código, aparecem Aparelho, Sistema, Endereço e Código, com Aprovar e Negar. Em pasta, a tela avisa que a pasta fica só neste aparelho e não tem versionamento de objeto. O desktop usa o mesmo conjunto de campos numa página local, aberta pelo item da bandeja.
