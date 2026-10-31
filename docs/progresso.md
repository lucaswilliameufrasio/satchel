# Progresso do desenvolvimento

Atualizado: 2026-10-31

O plano está em [plano.md](plano.md). Os passos 1 a 7 estão feitos. O que esta máquina Linux conseguiu fechar está em “Nesta máquina”. O que segue é o Mac.

## Aberto no Mac

Continuar a partir daqui:

- Aplicativo iOS e provedor de documentos.
- iPhone real: catálogo, pareamento, leitura do QR com a câmera do celular, chave do aparelho e marca.
- YubiKey no iPhone. O uso diário continua no cofre do sistema. A YubiKey embrulha a senha de recuperação com FIDO2 `hmac-secret` e não substitui o arquivo `keys/` do restic.

O QR já é desenhado no menu do desktop. A câmera do MacBook fica de fora. A leitura com a câmera do celular é o item acima.

## Não verificável nesta máquina

- YubiKey no desktop. Nenhuma estava ligada. Não há código que finja `hmac-secret`.
- WebDAV e SFTP continuam só com o aviso de pasta sem versionamento. O backend completo não foi feito.

## Fora do plano

OAuth, pasta grande, relay de push e apagamento remoto.

## Nesta máquina

- A bandeja publica a pasta com `publish_folder`, o mesmo caminho do teste Floci. O teste da bandeja usou um diretório local. Sincronizar sempre cobre os arquivos regulares. Pasta local avisa que não há versionamento de objeto.
- Aprovar exige senha de recuperação, credencial de manutenção e credencial do celular. A concessão leva a credencial do celular. Sem ela, nada é enviado e nenhuma chave nova aparece.
- A linha Devices lista as outras chaves. Revogar apaga a chave escolhida. A chave que abriu o repositório e a do próprio desktop ficam.
- O menu de adicionar aparelho tem uma linha QR desabilitada com o PNG do convite. O teste decodifica esse PNG de volta para o JSON do convite. O ícone do menu é pequeno.
- `probe_object_protection` consulta versionamento e ciclo de vida no Floci local, em HTTP path-style. `check_setup` recebe essa resposta. O teste exige versionamento ligado e ciclo de vida sem apagar objeto.
- No Floci, `publish_folder` grava o arquivo, a segunda chave abre o celular com `device-key-password`, e o `restic` 0.19.1 restaura com a senha de recuperação. Os dois hostnames em `keys/` são os ids opacos. O Floci tem uma credencial só (`test`). A política `phone_credential_policy` é o documento para anexar no bucket. Esse teste não cria um segundo principal.
- O celular abre um arquivo de concessão com a senha da chave do aparelho. `read_followed` lê o snapshot apontado pela marca Baixar agora. O teste do núcleo grava a cópia de trabalho e lê os mesmos bytes. A tela de entrada do Android fica atrás do extra `satchel.pair`. O catálogo de exemplo continua sendo a abertura normal.
- `gradle -p android test` passou. No emulador dedicado `satchel_api35`, sem janela, `NativeCatalogTest` passou os três testes. O catálogo de exemplo omite os bytes. Baixar agora grava `work/kept.txt`. O arquivo de concessão desse teste abre a única chave do exemplo. A chave distinta do aparelho continua no teste do host `grant_opens_with_the_device_password`.
- A tela, sem concessão, mostrou `kept.txt`, `2023-11-14T22:13:20Z`, o id do publicador e Ver. A pasta de trabalho estava vazia. Tocar em Baixar agora gravou `payload-bytes`, trocou o rótulo para Baixar agora e passou a listar os dois ids. O extra `satchel.pair` mostrou Endereço, Código e Impressão digital. O pareamento ao vivo não foi feito. O emulador foi desligado depois.

## Passos

| Passo | Estado | Evidência |
| --- | --- | --- |
| 1. Formato | feito | `cargo test` passou. O teste `official_restic_restores_a_snapshot_and_the_key_file_stays_opaque` grava um snapshot num diretório e o `restic` 0.19.1 restaura os mesmos bytes. O arquivo em `keys/` guarda o id opaco. |
| 2. Sincronização | feito | `cargo test` passou. O teste `two_clients_follow_see_once_always_delete_and_conflict` usa dois clientes num diretório local e um relógio controlado. Ver não baixa o arquivo. Baixar agora fica na primeira versão. Sincronizar sempre acompanha a pasta, inclusive arquivo novo. Apagar some com a cópia de trabalho e o snapshot anterior ainda tem os bytes. O conflito fica numa versão marcada, fora da fila. O cursor lido do snapshot traz o pai e o id opaco. |
| 3. Recuperação | feito | `cargo test` passou. `recovery_password_restores_the_same_bytes_as_official_restic` usa só a senha e o caminho, grava num diretório vazio e o `restic` 0.19.1 restaura os mesmos bytes. `check_setup` recusa S3, R2 e B2 sem versionamento ligado ou com regra de ciclo de vida que apaga objeto. Pasta local, WebDAV e SFTP seguem com aviso. A sonda do bucket não roda aqui: o teste entrega o estado já lido. |
| 4. Desktop | feito | `cargo test` passou. Uma pasta vira um perfil sem segredo no `profile.json`. O pareamento em `127.0.0.1` exige o código antes da aprovação. A tela mostra o nome, o sistema, o endereço do socket e a interface local, e ignora o endereço que o outro lado declara. Aprovar envia endpoint, credencial do bucket e senha da chave daquele aparelho. Negar não escreve concessão. A senha de recuperação não é campo. O cofre do sistema aceitou um ida e volta num serviço descartável. Sem display, o binário sai com código 2 e não registra a bandeja. Na sessão KWin já aberta, o Plasma registrou o ícone e o menu de adicionar aparelho. |
| 5. Android | feito | `gradle -p android test` passou. A primeira abertura lista o nome, a data e o aparelho, com o estado Ver, e a pasta de trabalho fica vazia. O teste no emulador `sampleOmitsFileBytesUntilReadFile` lê o mesmo repositório pelo núcleo e o JSON não contém os bytes. Baixar agora troca o rótulo na tela. A marca fica na sessão do aparelho e ainda não vira um snapshot novo. |
| 6. Chave do aparelho | feito | `cargo test` passou. `device_key_opens_without_the_recovery_password` grava a chave do aparelho, o `restic` 0.19.1 restaura com essa senha, e o arquivo em `keys/` guarda o id opaco. Apagar essa chave faz a senha do aparelho falhar. A senha de recuperação continua abrindo, e a chave que abriu o repositório não é apagada. Aprovar na bandeja grava a chave antes do convite. Sem a senha de recuperação no cofre, a aprovação não envia concessão. |
| 7. Marca do aparelho | feito | `cargo test` passou. `phone_mark_reopens_from_a_new_snapshot` grava Baixar agora num snapshot novo. Um processo novo lê essa marca, e o outro aparelho vê os dois ids. Ver não baixa e tira o aparelho da lista. O núcleo do Android chama essa gravação. O emulador não foi aberto de novo. |

As linhas da tabela são o registro de cada passo. O estado atual está em “Nesta máquina” e em “Aberto no Mac”.

## Notas

- 2026-10-30: plano registrado.
- 2026-10-30: passo 1 feito no diretório local. As opções S3 são montadas e testadas sem abrir um bucket.
- 2026-10-30: passo 2 feito com dois clientes headless. O estado publicado fica na descrição cifrada do snapshot. O conteúdo só é lido em baixar agora e sincronizar sempre.
- 2026-10-30: passo 3 feito. A recuperação não precisa do id do aparelho. A recusa de bucket sem versionamento está no núcleo e não abre a rede.
- 2026-10-30: passo 4 feito. `cargo fmt --all --check`, `cargo clippy --all-targets --offline -- -D warnings` e `cargo test --offline -- --test-threads=1` passaram. O código vem antes da aprovação. Código errado e impressão digital errada não chegam nela. O convite JSON tem `v`, `code`, `address` e `fingerprint`; um campo `recovery` é recusado. A imagem de QR não é desenhada. O anúncio mDNS só existe com a sessão aberta, só em interface local que não é loopback, malha ou ponte, e o TXT (`v`, `alias`, `fp`) foi conferido ao montar o registro. Nesta máquina o registro voltou e, ao soltar a sessão, a porta fechou. Nenhum segundo aparelho escutou o anúncio. Endereço digitado funciona sem mDNS. O teste do cofre usou o secret service da sessão, num serviço `satchel-test-…`, não no serviço `satchel`. Não houve diálogo na tela. Sem `DISPLAY` e sem `WAYLAND_DISPLAY`, `tray_binary_does_not_start_without_a_display` confirma a recusa.
- 2026-10-30: a bandeja abriu na sessão KWin existente, em `wayland-1`, com perfil temporário fora do diretório real. O Plasma listou o item, com título Satchel e ícone `folder-remote`. O menu mostrou a pasta e Add device. Add device mostrou código, endereço e impressão digital, e a porta passou a escutar. Cancel devolveu o menu anterior e fechou a porta. O processo foi encerrado e o item saiu do watcher. A tela inteira não foi fotografada.
- 2026-10-30: passo 5 feito. O catálogo sem marca passa a aparecer como Ver. `cargo test` inclui essa primeira olhada. No emulador dedicado `satchel_api35` (API 35, `x86_64`, sem janela), a tela mostrou `kept.txt`, `2023-11-14T22:13:20Z`, o id opaco e Ver. A pasta `cache/sample/work` ficou vazia. O teste instrumentado devolveu os bytes só em `readFile`. Tocar em Baixar agora mudou o rótulo para Baixar agora e a pasta de trabalho continuou vazia. Os três estados, inclusive a pasta, estão nos testes da sessão em JVM. A marca do celular não é gravada de volta no repositório. A biblioteca nativa não entra no git.
- 2026-10-30: o fluxo compartilhado passou num Floci local, imagem `floci/floci:2.3.0`, publicado só em `127.0.0.1`. O `cargo test` normal não sobe o Floci e não chama S3, R2 nem B2. O teste ignorado `desktop_publishes_and_the_phone_reads_metadata_only` criou o bucket, ligou o versionamento, viu que não havia ciclo de vida, gravou `notes/from-desktop.txt` e o segundo cliente listou só o metadado, em Ver. O `restic` 0.19.1 restaurou os mesmos bytes. O arquivo em `keys/` guarda o id opaco. Quem escreveu foi o núcleo, não a bandeja. Os dois lados abriram com a senha de recuperação de teste; não houve segunda chave de aparelho nem pareamento. No emulador `satchel_api35`, a tela mostrou o nome, `2023-11-14T22:13:20Z`, o id do desktop e Ver. A pasta `cache/shared-work` ficou sem o arquivo. Baixar agora trocou o rótulo e o arquivo continuou fora da pasta. `SharedStoreTest` devolveu os bytes. A marca do celular continua fora do repositório. O iPhone não foi testado.
- 2026-10-31: a bandeja foi conferida numa sessão KWin privada, `kwin_wayland --virtual`, 1920×1200, com HOME e D-Bus próprios. Sozinha, ela sai com “the tray did not start”, porque essa sessão não tem `StatusNotifierWatcher`. Com `kded6` e `plasmashell` só nessa sessão, o item registra título Satchel, ícone `folder-remote`, e o menu tem “No folder” e “Quit”: não havia perfil. O frame do compositor ficou preto. Um diálogo de prova apareceu no meio; painel e ícone não. A sessão Plasma que já estava aberta não foi usada.
- 2026-10-31: passo 6 feito num diretório local. O id do aparelho viaja no hello, depois do código, e não entra no convite. A senha de recuperação só embrulha a chave nova. A remoção dessa chave está no núcleo; a bandeja ainda não tem uma linha para revogar. A marca do celular continua fora do repositório. A bandeja continua sem publicar no bucket. O iPhone não foi testado.
- 2026-10-31: `cargo test` passa a subir o Floci `floci/floci:2.3.0` com testcontainers, no Docker rootless, publicado só em `127.0.0.1`. O container e os volumes anônimos saem no fim do teste. A imagem precisa já estar na máquina; o teste não baixa imagem e não lê `FLOCI_ENDPOINT`. S3, R2 e B2 reais continuam de fora.
- 2026-10-31: passo 7 feito num diretório local. A marca do celular é um snapshot com o id opaco desse aparelho. Reabrir lê Baixar agora, e o desktop passa a listar os dois aparelhos. Voltar para Ver tira o celular da lista e não apaga a cópia já baixada. `markSample` e `markStore` gravam essa marca. `gradle -p android test` passou. A bandeja continua sem publicar no bucket. O emulador e o iPhone não foram testados neste passo.
- 2026-10-31: o que cabia nesta máquina foi fechado. `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test -- --test-threads=1` passaram, inclusive o Floci local. No emulador `satchel_api35`, `NativeCatalogTest` passou os três testes. A tela mostrou o catálogo em Ver com a pasta de trabalho vazia. Baixar agora gravou a cópia. A tela de entrar mostrou os três campos. Não houve pareamento ao vivo. O iPhone, o aplicativo iOS e a YubiKey do iPhone continuam em “Aberto no Mac”. A YubiKey do desktop não estava ligada.
