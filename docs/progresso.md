# Progresso do desenvolvimento

Atualizado: 2026-10-30

O plano está em [plano.md](plano.md). Este arquivo é o registro do que já foi feito.

| Passo | Estado | Evidência |
| --- | --- | --- |
| 1. Formato | feito | `cargo test` passou. O teste `official_restic_restores_a_snapshot_and_the_key_file_stays_opaque` grava um snapshot num diretório e o `restic` 0.19.1 restaura os mesmos bytes. O arquivo em `keys/` guarda o id opaco. |
| 2. Sincronização | feito | `cargo test` passou. O teste `two_clients_follow_see_once_always_delete_and_conflict` usa dois clientes num diretório local e um relógio controlado. Ver não baixa o arquivo. Baixar agora fica na primeira versão. Sincronizar sempre acompanha a pasta, inclusive arquivo novo. Apagar some com a cópia de trabalho e o snapshot anterior ainda tem os bytes. O conflito fica numa versão marcada, fora da fila. O cursor lido do snapshot traz o pai e o id opaco. |
| 3. Recuperação | feito | `cargo test` passou. `recovery_password_restores_the_same_bytes_as_official_restic` usa só a senha e o caminho, grava num diretório vazio e o `restic` 0.19.1 restaura os mesmos bytes. `check_setup` recusa S3, R2 e B2 sem versionamento ligado ou com regra de ciclo de vida que apaga objeto. Pasta local, WebDAV e SFTP seguem com aviso. A sonda do bucket não roda aqui: o teste entrega o estado já lido. |
| 4. Desktop | feito | `cargo test` passou. Uma pasta vira um perfil sem segredo no `profile.json`. O pareamento em `127.0.0.1` exige o código antes da aprovação. A tela mostra o nome, o sistema, o endereço do socket e a interface local, e ignora o endereço que o outro lado declara. Aprovar envia endpoint, credencial do bucket e senha da chave daquele aparelho. Negar não escreve concessão. A senha de recuperação não é campo. O cofre do sistema aceitou um ida e volta num serviço descartável. Sem display, o binário sai com código 2 e não registra a bandeja. |
| 5. Android | pendente | — |

## Notas

- 2026-10-30: plano registrado.
- 2026-10-30: passo 1 feito no diretório local. As opções S3 são montadas e testadas sem abrir um bucket.
- 2026-10-30: passo 2 feito com dois clientes headless. O estado publicado fica na descrição cifrada do snapshot. O conteúdo só é lido em baixar agora e sincronizar sempre.
- 2026-10-30: passo 3 feito. A recuperação não precisa do id do aparelho. A recusa de bucket sem versionamento está no núcleo e não abre a rede.
- 2026-10-30: passo 4 feito. `cargo fmt --all --check`, `cargo clippy --all-targets --offline -- -D warnings` e `cargo test --offline -- --test-threads=1` passaram. O código vem antes da aprovação. Código errado e impressão digital errada não chegam nela. O convite JSON tem `v`, `code`, `address` e `fingerprint`; um campo `recovery` é recusado. A imagem de QR não é desenhada. O anúncio mDNS só existe com a sessão aberta, só em interface local que não é loopback, malha ou ponte, e o TXT (`v`, `alias`, `fp`) foi conferido ao montar o registro. Nesta máquina o registro voltou e, ao soltar a sessão, a porta fechou. Nenhum segundo aparelho escutou o anúncio. Endereço digitado funciona sem mDNS. O teste do cofre usou o secret service da sessão, num serviço `satchel-test-…`, não no serviço `satchel`. Não houve diálogo na tela. A bandeja não foi vista: `DISPLAY` e `WAYLAND_DISPLAY` estão vazios, e `tray_binary_does_not_start_without_a_display` só confirma a recusa.
