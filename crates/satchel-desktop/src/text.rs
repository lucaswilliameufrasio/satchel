//! The two languages of the desktop: English and Brazilian Portuguese.
//!
//! The tray follows the system language. The setup page follows the browser's
//! `Accept-Language` and falls back to the system. Anything that is not
//! Portuguese is shown in English.

use crate::Error;

/// A language the desktop speaks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Language {
    /// English, also the fallback.
    #[default]
    En,
    /// Brazilian Portuguese, for any `pt` tag.
    PtBr,
}

impl Language {
    /// `SATCHEL_LANG` when set, otherwise the system's preferred languages.
    pub fn detect() -> Self {
        if let Ok(forced) = std::env::var("SATCHEL_LANG")
            && let Some(language) = Self::from_tag(&forced)
        {
            return language;
        }
        Self::from_tags(sys_locale::get_locales())
    }

    /// The first tag that is English or Portuguese decides; none means English.
    pub fn from_tags<I, S>(tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        tags.into_iter()
            .find_map(|tag| Self::from_tag(tag.as_ref()))
            .unwrap_or_default()
    }

    /// `pt`, `pt-BR`, `pt_PT.UTF-8` are Portuguese; `en…` is English; others are neither.
    pub fn from_tag(tag: &str) -> Option<Self> {
        let primary = tag
            .split(['-', '_', '.', ';'])
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match primary.as_str() {
            "pt" => Some(Self::PtBr),
            "en" => Some(Self::En),
            _ => None,
        }
    }

    /// The language a browser asked for, by `q` weight, if it is one of ours.
    pub fn from_accept_language(header: &str) -> Option<Self> {
        let mut ranked: Vec<(u16, usize, Self)> = Vec::new();
        for (position, part) in header.split(',').enumerate() {
            let mut pieces = part.split(';');
            let tag = pieces.next().unwrap_or("").trim();
            let weight = pieces
                .find_map(|piece| piece.trim().strip_prefix("q="))
                .and_then(|q| q.trim().parse::<f32>().ok())
                .unwrap_or(1.0);
            if weight <= 0.0 {
                continue;
            }
            if let Some(language) = Self::from_tag(tag) {
                ranked.push(((weight * 1000.0) as u16, position, language));
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        ranked.first().map(|entry| entry.2)
    }

    /// The `lang` attribute of a page.
    pub fn html_tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::PtBr => "pt-BR",
        }
    }

    /// The English or the Portuguese text.
    pub(crate) fn pick<'a>(self, en: &'a str, pt: &'a str) -> &'a str {
        match self {
            Self::En => en,
            Self::PtBr => pt,
        }
    }
}

fn field_name(language: Language, field: &str) -> String {
    let (en, pt) = match field {
        "access_key_id" => ("Maintenance access key", "Chave de acesso de manutenção"),
        "secret_access_key" => ("Maintenance secret", "Segredo de manutenção"),
        "bucket" => ("Bucket", "Bucket"),
        "endpoint" => ("Endpoint", "Endpoint"),
        "region" => ("Region", "Região"),
        "prefix" => ("Prefix", "Prefixo"),
        "directory" => ("Repository folder", "Pasta do repositório"),
        "address" => ("Address", "Endereço"),
        other => (other, other),
    };
    language.pick(en, pt).to_string()
}

/// A sentence for the person: what went wrong and what to do. Never a secret.
pub fn describe(language: Language, err: &Error) -> String {
    use satchel_core::Error as Core;
    use satchel_pair::Error as Pair;
    let l = |en: &str, pt: &str| language.pick(en, pt).to_string();
    match err {
        Error::ProfileExists => l(
            "A folder is already set up on this device.",
            "Já existe uma pasta configurada neste aparelho.",
        ),
        Error::Path { path } => format!(
            "{} {}",
            l(
                "This path can't be used. Check that it exists and that you can write to it:",
                "Não dá para usar este caminho. Confira se ele existe e se você pode gravar nele:",
            ),
            path.display()
        ),
        Error::Profile => l(
            "The settings file of this device can't be read.",
            "O arquivo de configuração deste aparelho não pôde ser lido.",
        ),
        Error::AlreadyPairing => l(
            "A device is already being added from the tray.",
            "Já há um aparelho sendo adicionado pela bandeja.",
        ),
        Error::MissingSecret => l(
            "The storage key is not in the keychain. Set up this device again.",
            "A chave do armazenamento não está no chaveiro. Configure este aparelho de novo.",
        ),
        Error::MissingPhoneCredential => l(
            "The device key is not in the keychain. Set up this device again.",
            "A chave dos aparelhos não está no chaveiro. Configure este aparelho de novo.",
        ),
        Error::PhoneCredential => l(
            "The device key can't be sent. Check the device access key and secret.",
            "Não dá para enviar a chave dos aparelhos. Confira a chave e o segredo dos aparelhos.",
        ),
        Error::MissingRecovery => l(
            "The recovery password is not in this device's keychain.",
            "A senha de recuperação não está no chaveiro deste aparelho.",
        ),
        Error::Store { kind } => format!(
            "{} ({kind})",
            l(
                "The keychain refused the request. Unlock it and try again",
                "O chaveiro recusou o pedido. Desbloqueie e tente de novo",
            )
        ),
        Error::Grant => l(
            "What the other device sent didn't open the repository. Add this device again.",
            "O que o outro aparelho enviou não abriu o repositório. Adicione este aparelho de novo.",
        ),
        Error::NotMaintainer => l(
            "Only the device that created the repository can change the recovery password.",
            "Só o aparelho que criou o repositório pode trocar a senha de recuperação.",
        ),
        Error::Field { field } | Error::Device(Core::EmptyField { field }) => format!(
            "{} {}.",
            l("Fill in", "Preencha"),
            field_name(language, field)
        ),
        Error::Tray => l("The tray didn't start.", "A bandeja não abriu."),
        Error::Io => l(
            "Couldn't read or write a file on this device.",
            "Não foi possível ler ou gravar um arquivo neste aparelho.",
        ),
        Error::Pair(pair) => match pair {
            Pair::Code => l(
                "The code must have 6 digits.",
                "O código deve ter 6 dígitos.",
            ),
            Pair::CodeRejected => l(
                "The code didn't match. Check it on the other device and try again.",
                "O código não confere. Confira no outro aparelho e tente de novo.",
            ),
            Pair::Fingerprint => l(
                "The fingerprint doesn't match. Copy it again from the other device.",
                "A impressão digital não confere. Copie de novo do outro aparelho.",
            ),
            Pair::TimedOut => l(
                "Nobody approved in time. Open Add device again and try again.",
                "Ninguém aprovou a tempo. Abra Adicionar aparelho de novo e tente outra vez.",
            ),
            Pair::Denied => l(
                "The other device denied the request.",
                "O outro aparelho recusou o pedido.",
            ),
            Pair::Closed => l(
                "The invite was closed. Open Add device again.",
                "O convite foi fechado. Abra Adicionar aparelho de novo.",
            ),
            Pair::Version => l(
                "The other device runs another version of Satchel. Update both.",
                "O outro aparelho tem outra versão do Satchel. Atualize os dois.",
            ),
            Pair::Network => l(
                "Couldn't reach the other device. Check that both are on the same network.",
                "Não foi possível falar com o outro aparelho. Confira se os dois estão na mesma rede.",
            ),
            Pair::Message | Pair::Tls => l(
                "Pairing failed. Try again.",
                "O pareamento falhou. Tente de novo.",
            ),
        },
        Error::Device(core) => match core {
            Core::ObjectProtection => l(
                "The bucket needs versioning on and no lifecycle rule that deletes objects.",
                "O bucket precisa de versionamento ligado e de nenhuma regra de ciclo de vida que apague objetos.",
            ),
            Core::ProbeDenied => l(
                "The storage won't let this key read the bucket's versioning. Give it bucket \
                 permissions. R2 doesn't offer versioning over the S3 API and is always refused.",
                "O armazenamento não deixa esta chave ler o versionamento do bucket. Dê a ela \
                 permissões no bucket. O R2 não oferece versionamento pela API S3 e sempre recusa.",
            ),
            Core::Probe => l(
                "Couldn't check the bucket. Check the endpoint, the region and the network.",
                "Não foi possível conferir o bucket. Confira o endpoint, a região e a rede.",
            ),
            Core::RepositoryExists => l(
                "A repository already exists there. Join it from the device that holds it.",
                "Já existe um repositório ali. Entre nele pelo aparelho que o mantém.",
            ),
            Core::SamePassword => l(
                "The new password is the same as the current one.",
                "A senha nova é igual à atual.",
            ),
            Core::UnsupportedStorage => l(
                "This storage can't be set up yet.",
                "Este armazenamento ainda não pode ser configurado.",
            ),
            Core::FolderUnavailable => l(
                "The folder looks empty or unreachable, so nothing was deleted.",
                "A pasta parece vazia ou inacessível, então nada foi apagado.",
            ),
            Core::SourceDirectory { path } => format!(
                "{} {}",
                l("This is not a folder:", "Isto não é uma pasta:"),
                path.display()
            ),
            Core::Repository { message } => format!(
                "{} {message}",
                l(
                    "The storage refused the request:",
                    "O armazenamento recusou o pedido:"
                )
            ),
            other => format!("{} {other}", l("Something went wrong:", "Algo deu errado:")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_portuguese_tag_is_portuguese_and_anything_else_is_english() {
        assert_eq!(Language::from_tags(["pt-BR"]), Language::PtBr);
        assert_eq!(Language::from_tags(["pt_PT.UTF-8"]), Language::PtBr);
        assert_eq!(Language::from_tags(["fr-FR", "pt-BR"]), Language::PtBr);
        assert_eq!(Language::from_tags(["en-US", "pt-BR"]), Language::En);
        assert_eq!(Language::from_tags(["de-DE"]), Language::En);
        assert_eq!(Language::from_tags(Vec::<String>::new()), Language::En);
    }

    #[test]
    fn the_browser_weights_decide() {
        assert_eq!(
            Language::from_accept_language("pt-BR,pt;q=0.9,en-US;q=0.8"),
            Some(Language::PtBr)
        );
        assert_eq!(
            Language::from_accept_language("en;q=0.5, pt;q=0.7"),
            Some(Language::PtBr)
        );
        assert_eq!(
            Language::from_accept_language("fr-FR,en;q=0.3"),
            Some(Language::En)
        );
        assert_eq!(Language::from_accept_language("fr-FR,de"), None);
        assert_eq!(
            Language::from_accept_language("pt;q=0,en"),
            Some(Language::En)
        );
        assert_eq!(Language::from_accept_language(""), None);
    }

    #[test]
    fn errors_read_as_sentences_in_both_languages() {
        let err = Error::Pair(satchel_pair::Error::CodeRejected);
        assert!(describe(Language::En, &err).starts_with("The code didn't match"));
        assert!(describe(Language::PtBr, &err).starts_with("O código não confere"));
        let empty = Error::Device(satchel_core::Error::EmptyField { field: "bucket" });
        assert_eq!(describe(Language::En, &empty), "Fill in Bucket.");
        assert_eq!(describe(Language::PtBr, &empty), "Preencha Bucket.");
    }
}
