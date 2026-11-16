# Plano de desenvolvimento

Produto de sincronização cifrada e versionada de arquivos e pastas genéricos, no storage escolhido pela pessoa. O formato é o repositório restic, via `rustic`, para que o `restic` oficial continue sendo a saída de emergência. Não há servidor obrigatório e não há conta do aplicativo.

O `.kdbx` foi o caso que revelou a necessidade. O produto não sabe o que é um KeePass.

## Decisões fechadas

- Um perfil é uma pasta e um repositório.
- O núcleo é Rust. O desktop entra depois, com bandeja. O celular entra depois do núcleo cumprir as regras de sincronização.
- A senha de recuperação é aleatória, vira um arquivo normal em `keys/` e fica num gerenciador de senhas que abre sem este repositório. O `restic` oficial usa essa senha.
- YubiKeys são opcionais. Cada uma embrulha a mesma senha de recuperação com FIDO2 `hmac-secret`. O uso diário abre pelo cofre do sistema. No iPhone isso fica para um spike posterior.
- O arquivo `keys/` do restic recebe um identificador opaco, não o nome da máquina. Esse arquivo também não leva `created`: hostname, username e created ficam legíveis para quem lista `keys/`.
- Apelido, sistema e a data de registro viajam no estado cifrado do snapshot daquele aparelho. O último envio é a data do snapshot mais novo desse id. Um apelido novo não move a data de registro. Sem esses campos, a tela omite a linha em vez de inventar.
- O cursor da sincronização vive no snapshot, com o pai e o id do aparelho. O SQLite do aparelho é cache.
- Na v1 entram só arquivos regulares, lidos de uma cópia estável.
- A credencial do celular lista, lê e grava, e só apaga os próprios locks. Apagar pack, índice, snapshot e chave fica na credencial do desktop.
- Em S3, R2 e B2 o setup só continua com versionamento ligado e sem regra de ciclo de vida apagando objeto. Em pasta local, WebDAV e SFTP o setup avisa que essa proteção não existe.
- Cache, SQLite e cofre local ficam fora do backup do iCloud e do Google. A pasta sincronizada não inclui a configuração do aplicativo nem o kit de recuperação.
- Revogar um aparelho perdido corta o acesso futuro. Não apaga as cópias já abertas nele.
- Trocar a senha de recuperação reembrulha todas as YubiKeys na mesma sessão.
- A configuração só está pronta quando um aparelho zerado abre o repositório pelo aplicativo e pelo `restic` oficial.
- Decidido em 2026-11-16: cada aparelho recebe a própria chave de bucket, criada pelo aparelho principal com a chave de manutenção (IAM no S3; no B2, chave de aplicação com prefixo; no R2, token da Cloudflare limitado ao bucket, quando a pessoa dá um token que cria tokens). Remover um aparelho apaga a chave dele no provedor. Sem permissão para criar chaves, o aparelho recebe a chave compartilhada e a tela diz que remover não corta o bucket.
- Decidido em 2026-11-16: em armazenamento de objetos, só aparelhos principais adicionam aparelhos. A pessoa define o grupo: qualquer aparelho vira principal com a senha de recuperação e a chave de manutenção.
- Decidido em 2026-11-16: a linguagem visual segue a direção C de [referencias-visuais.md](referencias-visuais.md) (superfície e tom do Mercury, status e dados do Geist).

### Pareamento

Quem entra digita um código. No celular, também pode ler o QR mostrado no desktop. O desktop desenha esse QR e não abre câmera. Quem já tem o repositório aprova e vê o nome do aparelho, o sistema, o endereço na malha e um código curto desta tentativa.

O código só autentica. Os segredos seguem depois do aceite: endpoint, credencial do celular e senha da chave daquele aparelho. A senha de recuperação e a credencial de manutenção do desktop ficam de fora. Em objeto, bucket, região e prefixo viajam dentro da credencial do celular. A política dessa credencial permite listar, ler e gravar, e apagar só objetos em `locks/` com id de 64 hex. Pack, índice, snapshot, chave, versionamento e ciclo de vida ficam negados. Quem administra o bucket anexa esse documento. Um armazenamento sem política de identidade não aplica isso sozinho.

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

### 7. Marca do aparelho

A marca escolhida num aparelho vira um snapshot novo. Outro processo desse aparelho lê Ver, Baixar agora ou Sincronizar sempre desse snapshot. Baixar agora grava a cópia uma vez. Sincronizar sempre coloca o aparelho na lista de quem mantém o arquivo. Ver não baixa.

## Fora destes passos

Provedor de documentos do iOS, YubiKey no iPhone, OAuth, pasta grande, relay de push e apagamento remoto.

## Próxima prioridade proposta: experiência e confiança

Antes de acrescentar outra funcionalidade grande, priorizar a jornada de
configurar o Satchel, conectar outro aparelho e entender se os arquivos estão
seguros e sincronizados. Esta é uma priorização proposta, não uma mudança nas
regras de segurança nem uma aprovação da identidade visual.

1. **Diagnosticar a jornada real.** Percorrer primeira abertura, configuração
   do armazenamento, conexão de outro aparelho, sincronização, falha e
   recuperação em desktop e celular. Capturar as telas e seus estados vazios,
   de espera, erro e sucesso; registrar o que foi observado e o que não pôde
   ser exercitado.
2. **Redesenhar a jornada principal.** Aplicar hierarquia visual, agrupamento,
   reconhecimento em vez de memorização, prevenção de erro e visibilidade do
   estado. Deixar claros o estado e a última confirmação da sincronização,
   sem prometer proteção ou sincronização que não foram comprovadas. Priorizar
   o QR no pareamento e explicar a entrada manual. Tratar senha de recuperação,
   troca de pasta e remoção de aparelho conforme suas consequências.
3. **Definir e provar a linguagem visual.** A proposta existente em
   [identidade-visual.md](identidade-visual.md) ainda precisa de aprovação.
   Antes de escolher uma nova direção visual, obter 2 a 5 referências de
   aplicativos ou telas escolhidas pelo responsável pelo produto. Avaliar
   capturas reais, inclusive tamanhos de texto, modo escuro, leitor de tela e
   estados da jornada. Depois estender a linguagem aprovada às demais telas.
4. **Fechar lacunas que afetam a confiança.** Resolver ou comunicar com
   precisão a limitação da revogação da credencial compartilhada do bucket;
   validar o comportamento de várias pastas e o pareamento e a sincronização
   entre aparelhos reais. A solução de revogação ainda não está decidida.
5. **Preparar a entrega.** Atualizar a descrição da PR com seu escopo real e
   executar as verificações aplicáveis no commit final. Separar resultados
   comprovados de testes que dependem de dispositivos ou armazenamento real.

O percurso de 2026-11-16, o que mudou e o que segue pendente estão em
[progresso.md](progresso.md#experiência-e-confiança). Os itens 3 (referências
visuais) e 4 (solução da chave do bucket) dependem de decisão de quem
responde pelo produto. YubiKey, OAuth, push e apagamento remoto
continuam fora ou incompletos; sua prioridade frente a essa jornada não foi
decidida.

## Licença

Apache-2.0 OR MIT, a mesma faixa do `rustic`.
