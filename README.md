# satchel

Sincronização cifrada e versionada de arquivos e pastas no storage da própria pessoa. O formato do repositório é o do [restic](https://restic.net/), para que a recuperação também funcione com o cliente oficial.

Não há servidor obrigatório. Cada aparelho fala com o storage. O pareamento entre aparelhos só entrega a configuração.

O plano e o andamento estão em [`docs/plano.md`](docs/plano.md) e [`docs/progresso.md`](docs/progresso.md).

O núcleo está em `crates/satchel-core`. O pareamento está em `crates/satchel-pair`. A bandeja está em `crates/satchel-desktop`. O catálogo Android está em `android/`, e a ponte com o núcleo está em `crates/satchel-ffi`. As verificações estão em [`TESTING_STANDARD.md`](TESTING_STANDARD.md).
