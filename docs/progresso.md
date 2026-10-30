# Progresso do desenvolvimento

Atualizado: 2026-10-30

O plano está em [plano.md](plano.md). Este arquivo é o registro do que já foi feito.

| Passo | Estado | Evidência |
| --- | --- | --- |
| 1. Formato | feito | `cargo test` passou. O teste `official_restic_restores_a_snapshot_and_the_key_file_stays_opaque` grava um snapshot num diretório e o `restic` 0.19.1 restaura os mesmos bytes. O arquivo em `keys/` guarda o id opaco. |
| 2. Sincronização | pendente | — |
| 3. Recuperação | pendente | — |
| 4. Desktop | pendente | — |
| 5. Android | pendente | — |

## Notas

- 2026-10-30: plano registrado.
- 2026-10-30: passo 1 feito no diretório local. As opções S3 são montadas e testadas sem abrir um bucket.
