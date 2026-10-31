# Plano de desenvolvimento

Produto de sincronização cifrada e versionada de arquivos e pastas genéricos, no storage escolhido pela pessoa. O formato é o repositório restic, via `rustic`, para que o `restic` oficial continue sendo a saída de emergência. Não há servidor obrigatório e não há conta do aplicativo.

O `.kdbx` foi o caso que revelou a necessidade. O produto não sabe o que é um KeePass.

## Decisões fechadas

- Um perfil é uma pasta e um repositório.
- O núcleo é Rust. O desktop entra depois, com bandeja. O celular entra depois do núcleo cumprir as regras de sincronização.
- A senha de recuperação é aleatória, vira um arquivo normal em `keys/` e fica num gerenciador de senhas que abre sem este repositório. O `restic` oficial usa essa senha.
- YubiKeys são opcionais. Cada uma embrulha a mesma senha de recuperação com FIDO2 `hmac-secret`. O uso diário abre pelo cofre do sistema. No iPhone isso fica para um spike posterior.
- O arquivo `keys/` do restic recebe um identificador opaco, não o nome da máquina.
- O cursor da sincronização vive no snapshot, com o pai e o id do aparelho. O SQLite do aparelho é cache.
- Na v1 entram só arquivos regulares, lidos de uma cópia estável.
- A credencial do celular lista, lê e grava, e só apaga os próprios locks. Apagar pack, índice, snapshot e chave fica na credencial do desktop.
- Em S3, R2 e B2 o setup só continua com versionamento ligado e sem regra de ciclo de vida apagando objeto. Em pasta local, WebDAV e SFTP o setup avisa que essa proteção não existe.
- Cache, SQLite e cofre local ficam fora do backup do iCloud e do Google. A pasta sincronizada não inclui a configuração do aplicativo nem o kit de recuperação.
- Revogar um aparelho perdido corta o acesso futuro. Não apaga as cópias já abertas nele.
- Trocar a senha de recuperação reembrulha todas as YubiKeys na mesma sessão.
- A configuração só está pronta quando um aparelho zerado abre o repositório pelo aplicativo e pelo `restic` oficial.

### Pareamento

Quem entra digita um código. No celular, também pode ler o QR mostrado no desktop. Quem já tem o repositório aprova e vê o nome do aparelho, o sistema, o endereço na malha e um código curto desta tentativa.

O código só autentica. Os segredos seguem depois do aceite: endpoint, credencial do bucket e senha da chave daquele aparelho. A senha de recuperação não entra no convite.

Na rede local, o anúncio mDNS fica ligado só enquanto a tela de adicionar aparelho está aberta. O anúncio leva versão, apelido aleatório, porta e a impressão digital do certificado. A impressão conferida é a do TLS da conexão. O PAKE acontece antes da tela de aprovação. Pelo NetBird ou pela Tailscale, o convite inclui o endereço escrito. O multicast não atravessa a malha.

### Catálogo e sincronização

Cada arquivo ou pasta tem um destes estados:

- **Ver.** Nome, tamanho, data da última versão e aparelhos que mantêm cópia. O conteúdo fica no storage.
- **Baixar agora.** Traz o arquivo uma vez e não acompanha mudanças posteriores.
- **Sincronizar sempre.** Baixa, envia e acompanha. Arquivo novo dentro de uma pasta marcada entra nesse estado.

Apagar num aparelho remove a cópia de trabalho nos outros. A versão antiga fica no histórico e só volta por restauração.

Conflito sobe como versão marcada. Não ocupa o lugar do arquivo principal e não volta para a fila.

A lista de aparelhos é a do pareamento. Cada aparelho publica, cifrado, o que sincroniza.

## Passos

Cada passo termina com o arquivo `docs/progresso.md` atualizado e um commit.

### 1. Formato

Um núcleo Rust cria um repositório, grava um snapshot de arquivos regulares e o `restic` oficial lê esse snapshot de volta com a senha de recuperação. O identificador gravado em `keys/` é opaco.

A prova local usa um diretório. O backend S3 fica no mesmo núcleo, e a prova contra um bucket real depende de credencial que não entra no repositório.

### 2. Sincronização

Dois clientes, sem interface, cumprem ver, baixar agora, sincronizar sempre, apagar a cópia de trabalho e subir o conflito como versão marcada. O cursor está no snapshot. O teste usa storage local e relógio controlado.

### 3. Recuperação

Um diretório vazio, só com a senha de recuperação e o caminho do repositório, abre os mesmos bytes pelo núcleo e pelo `restic` oficial. Quando o backend for S3, R2 ou B2, o setup recusa bucket sem versionamento.

### 4. Desktop

Bandeja, cofre do sistema e uma pasta. O pareamento oferece código, descoberta só com a tela aberta, ou endereço no NetBird ou na Tailscale. A aprovação mostra quem está pedindo.

### 5. Android

A primeira abertura mostra o catálogo, sem baixar o conteúdo. Os três estados funcionam nos arquivos marcados.

### 6. Chave do aparelho

Aprovar grava um arquivo em `keys/` para a senha daquele aparelho. O id no arquivo é o id opaco que o aparelho enviou depois do código, não o nome. O aparelho abre o repositório com essa senha. O `restic` oficial também. A senha de recuperação continua abrindo e não entra no convite. Revogar apaga esse arquivo, e a senha do aparelho deixa de abrir.

## Fora destes passos

Provedor de documentos do iOS, YubiKey no iPhone, OAuth, pasta grande, relay de push e apagamento remoto.

## Licença

Apache-2.0 OR MIT, a mesma faixa do `rustic`.
