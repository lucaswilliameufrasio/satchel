# Referências visuais candidatas (2026-11-16)

Pesquisa para o passo 3 do plano. Nada aqui foi aprovado: quem decide o produto escolhe de 2 a 5 referências. Pontos de partida declarados: Mercury (visual geral) e Vercel/Geist (gráficos e métricas).

| # | Referência | Informa no Satchel | O que levar |
| --- | --- | --- | --- |
| 1 | [Tailscale: adicionar por QR](https://tailscale.com/kb/1336/device-add-qr-code) | Parear celular | Dizer quem lê o QR; tela “aguardando aprovação” |
| 2 | [Tailscale: aprovação](https://tailscale.com/kb/1099/device-approval) | Aprovar, lista de aparelhos | Pendentes no topo; aprovar em um passo |
| 3 | [1Password Emergency Kit](https://support.1password.com/emergency-kit/) | Senha de recuperação | Kit para imprimir/salvar; onde guardar |
| 4 | [Syncthing GUI](https://docs.syncthing.net/intro/gui.html) | Painel | Vocabulário de estados; quanto falta |
| 5 | [Dropbox Sync & storage](https://help.dropbox.com/sync/sync-storage-dashboard) | Painel, catálogo | Separar “acontecendo” de “precisa de você” |
| 6 | [Geist Status Dot](https://vercel.com/geist/status-dot) | Linhas de arquivo e aparelho | Só “em andamento” anima; rótulo + tempo relativo |
| 7 | [Linear: redesign](https://linear.app/now/how-we-redesigned-the-linear-ui) | Tema claro/escuro | Tema de 3 variáveis; contraste ajustável |
| 8 | [Geist cores](https://vercel.com/geist/colors) | Escala de cor | 10 passos por função (fundo, borda, texto) |

Links 1 a 8 responderam 200 em 2026-11-16. Shots do Mercury no Dribbble e telas do Mobbin não puderam ser vistos (bloqueio de robô / login).

## Direções

- **A. Mercury calmo:** neutros levemente azulados (claro `#fbfcfd`/texto `#272735`; escuro `#10101a`/texto `#ededf3`), cartões de 12px, estado primeiro, semânticos suaves (verde `#188554`, laranja `#c45000`, vermelho `#d03275`, acento `#5266eb`).
- **B. Geist preciso:** neutros puros (`#ffffff`/`#171717`; `#0a0a0a`/`#ededed`), bordas de 1px, raio 6px, Geist Sans + Mono, pontos de status com rótulo.
- **C. Mistura (sugerida):** superfície, espaço e tom da A; status, escala de cor e mono em metadados da B. Fonte do sistema nos apps (Dynamic Type/fontScale), Inter ou Geist na página.

Em todas: estado nunca só por cor, e pareamento no modelo do Tailscale (QR → aguardando aprovação → aprovar no aparelho que já está no repositório).
