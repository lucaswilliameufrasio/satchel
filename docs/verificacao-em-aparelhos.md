# Verificação em aparelhos e contas reais

O que falta provar do plano e não cabe num simulador nem num emulador. Cada item diz o que fazer e o que anotar em [progresso.md](progresso.md).

## 1. Chave por aparelho cortada de verdade

Precisa de uma conta com um bucket versionado e uma chave de manutenção com permissão para criar chaves.

- **AWS S3:** a chave de manutenção com `iam:CreateUser`, `iam:PutUserPolicy`, `iam:CreateAccessKey`, `iam:ListAccessKeys`, `iam:DeleteAccessKey`, `iam:DeleteUserPolicy` e `iam:DeleteUser`.
- **B2:** a chave de manutenção com `writeKeys`.
- **R2:** um token da Cloudflare que cria tokens (Usuário → API Tokens → Edit), no campo opcional.

Passos:

1. No desktop, crie o repositório nesse bucket.
2. Adicione um segundo aparelho. A aprovação deve terminar com “Pronto” sem a frase da chave compartilhada.
3. No segundo aparelho, sincronize uma vez.
4. No desktop, abra Remover um aparelho. O aparelho deve aparecer com “Ele tem chave própria no bucket”. Remova.
5. No segundo aparelho, sincronize de novo. Deve falhar.
6. No console do provedor, confira que o usuário IAM, a chave de aplicação ou o token `satchel-…` sumiu.

Anote: provedor, se o passo 5 falhou e a mensagem que apareceu.

## 2. Leitor de tela

1. **iPhone (VoiceOver):** percorra primeira abertura, Entrar neste aparelho, Criar repositório, a senha de recuperação, o convite e a aprovação.
2. **Android (TalkBack):** o mesmo percurso.
3. Em cada tela, confira que o título é lido como cabeçalho, que cada botão diz o que faz e que a senha e o código são lidos em grupos.

Anote: aparelho, sistema e qualquer elemento lido errado ou pulado.

## 3. Câmera e QR

1. No desktop, abra Adicionar aparelho.
2. No celular, toque em Entrar neste aparelho e depois em Ler QR, e aponte para o QR.
3. Os campos devem ser preenchidos, e a aprovação no desktop deve mostrar o mesmo código dos dois lados.

Anote: aparelho, se leu na primeira tentativa e quanto tempo levou.
