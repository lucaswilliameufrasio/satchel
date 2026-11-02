# Catálogo no telefone

Atualizado: 2026-11-01

A apresentação do catálogo no iOS e no Android. O Android deste checkout não foi alterado: a implementação fica no outro computador, seguindo este texto. O iOS em `ios/Satchel` é a referência que já roda.

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

Sem arquivos: Nenhum arquivo no catálogo.

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

Falha: Não foi possível entrar. O texto do QR rejeitado não aparece.

Ações: Ler QR e Entrar. Ler o QR preenche os campos e volta para esta tela. A pessoa ainda toca Entrar. Nada entra sozinho depois do scan.

Câmera, título Ler QR: Aponte a câmera para o QR do desktop.

## Estados vazios

Carregando, centralizado: Lendo o catálogo…

Falha ao abrir, centralizado: Não foi possível abrir o catálogo.

## Logo

`assets/satchel-icon.png` é a marca, 1024×1024, sem texto. O iOS usa essa imagem como ícone único do app. A bandeja do macOS usa `assets/satchel-tray.rgba`, 36×36, RGB preto e o alfa da marca, como template. O Linux continua com o nome de ícone `folder-remote`.

## Android

Aplicar o mesmo texto e a mesma hierarquia em Compose. Não manter o hex na linha da lista. O teste que hoje exige o id na linha passa a exigi-lo na formatação da tela do arquivo. Na tela do arquivo, o id local leva o título Este aparelho e o hex agrupado fica embaixo. Cada aparelho também mostra apelido, sistema, Registrado e Último envio quando o JSON `holders` trouxer `name`, `os`, `enrolledUnix` e `lastUnix`. Campo ausente não vira texto. No exemplo, esse id é o leitor `fedcba9876543210fedcba9876543210`, e o publicador vem como Publicador. Com concessão, comparar com o `device-id` já gravado, sem criar o arquivo. O apelido digitado na entrada precisa ser enviado no mark, senão o snapshot deste aparelho sai sem nome. Mark sem apelido continua válido: o JSON desse aparelho traz o id e `lastUnix`, sem `name`. Copiar `assets/satchel-icon.png` para o mipmap. Este checkout não cria esse mipmap.

Funções que este desenho substitui em `android/.../catalog/Catalog.kt`: o apoio da linha deixa de ser `rowDetails`. `rowWhen` deixa de fixar UTC e formata `versionUnix` no fuso do aparelho, `dd/MM/yyyy HH:mm` mais o nome curto do fuso. O teste que espera `14/11/2023 22:13 UTC` passa a usar um `ZoneId` explícito, para não depender do fuso da máquina. O id agrupado e a contagem de aparelhos são novos. `followLabel` permanece.

Este Mac não cria emulador Android para esta tela.

## Prova no simulador

No iPhone 17, a lista mostrou `kept.txt`, o tamanho, a data e a contagem de aparelhos, com o estado na mesma linha do nome. Com o fuso do aparelho, o mesmo instante apareceu como `14/11/2023 19:13 GMT-3`. A data não quebrou quando o estado passou a Baixar agora. A tela do arquivo mostrou tamanho, data, as três frases e os ids em grupos de quatro. O teste tocou Baixar agora: o checkmark foi para essa linha e a lista passou a dizer Baixar agora e 2 aparelhos. A câmera não abriu.

Na passagem seguinte, a tela de `kept.txt` mostrou o outro id agrupado e, embaixo, Este aparelho com `fedc ba98 7654 3210 fedc ba98 7654 3210`. O teste de interface achou esse título. A tela inicial do simulador mostrou o ícone da bolsa com o nome Satchel. A mesma marca apareceu na tela inicial do iPhone de Lucas. Abrir o app nesse aparelho pediu verificação do desenvolvedor na rede e o catálogo não abriu. A bandeja do macOS não foi iniciada, porque esse processo usa o serviço de cofre `satchel`. A silhueta de 36×36 foi conferida à parte, e o teste `menu_bar_mark_is_a_36_pixel_template` passou.

Na passagem dos cartões, antes do toque, `kept.txt` mostrou Publicador, Sistema exemplo, Registrado `14/11/2023 19:13 GMT-3`, Último envio na mesma data e o id agrupado do publicador. O primeiro toque em Baixar agora caiu em “Não foi possível abrir o catálogo.”, porque a ABI em C recusava apelido vazio. O mark passou a aceitar apelido e sistema vazios. No host, `empty_device_label_still_marks_the_sample` passou. No simulador, o toque seguinte marcou Baixar agora, manteve o cartão do Publicador e mostrou Este aparelho só com Último envio `14/11/2023 19:13 GMT-3` e o id do leitor. A lista voltou com Baixar agora e 2 aparelhos. A câmera não abriu. A build iphoneos foi assinada, `codesign --verify --deep --strict` passou, e `devicectl device install app` instalou de novo em `com.lucaswilliameufrasio.satchel.catalogcheck`. A tela do iPhone estava bloqueada, então esses cartões não foram vistos no aparelho.
