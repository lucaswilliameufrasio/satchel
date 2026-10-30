# Progresso do desenvolvimento

Atualizado: 2026-10-30

O plano está em [plano.md](plano.md). Este arquivo é o registro do que já foi feito.

| Passo | Estado | Evidência |
| --- | --- | --- |
| 1. Formato | feito | `cargo test` passou. O teste `official_restic_restores_a_snapshot_and_the_key_file_stays_opaque` grava um snapshot num diretório e o `restic` 0.19.1 restaura os mesmos bytes. O arquivo em `keys/` guarda o id opaco. |
| 2. Sincronização | feito | `cargo test` passou. O teste `two_clients_follow_see_once_always_delete_and_conflict` usa dois clientes num diretório local e um relógio controlado. Ver não baixa o arquivo. Baixar agora fica na primeira versão. Sincronizar sempre acompanha a pasta, inclusive arquivo novo. Apagar some com a cópia de trabalho e o snapshot anterior ainda tem os bytes. O conflito fica numa versão marcada, fora da fila. O cursor lido do snapshot traz o pai e o id opaco. |
| 3. Recuperação | pendente | — |
| 4. Desktop | pendente | — |
| 5. Android | pendente | — |

## Notas

- 2026-10-30: plano registrado.
- 2026-10-30: passo 1 feito no diretório local. As opções S3 são montadas e testadas sem abrir um bucket.
- 2026-10-30: passo 2 feito com dois clientes headless. O estado publicado fica na descrição cifrada do snapshot. O conteúdo só é lido em baixar agora e sincronizar sempre.
