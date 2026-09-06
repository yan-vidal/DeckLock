use fluent_bundle::{FluentBundle, FluentResource};
use std::path::Path;

pub struct I18n {
    catalogs: Vec<FluentBundle<FluentResource>>,
}

impl I18n {
    pub fn new(locale: Option<&str>, override_dir: Option<&Path>) -> Result<Self, String> {
        let env_locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()));
        let normalized = locale
            .or(env_locale.as_deref())
            .unwrap_or("en-US")
            .split(['.', '@'])
            .next()
            .unwrap_or("en-US")
            .replace('_', "-");
        let language = normalized
            .split('-')
            .next()
            .unwrap_or("en")
            .to_ascii_lowercase();
        // Arbitrary external locales are supported; built-in Portuguese covers pt variants.
        let selected = if language == "pt" {
            "pt-BR".to_owned()
        } else if language == "en" || normalized == "C" || normalized == "POSIX" {
            "en-US".to_owned()
        } else {
            normalized
        };
        let locale_id: unic_langid::LanguageIdentifier = selected
            .parse()
            .map_err(|e| format!("Invalid locale {selected}: {e}"))?;
        let mut catalogs = Vec::new();
        if let Some(dir) = override_dir {
            let path = dir.join(format!("{selected}.ftl"));
            match std::fs::read_to_string(&path) {
                Ok(source) => catalogs.push(bundle(locale_id.clone(), source)?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
        }
        if selected == "pt-BR" {
            catalogs.push(bundle(
                locale_id,
                include_str!("../locales/pt-BR.ftl").into(),
            )?);
        }
        catalogs.push(bundle(
            "en-US".parse().unwrap(),
            include_str!("../locales/en-US.ftl").into(),
        )?);
        Ok(Self { catalogs })
    }

    pub fn text(&self, key: &str) -> String {
        for catalog in &self.catalogs {
            if let Some(pattern) = catalog.get_message(key).and_then(|message| message.value()) {
                let mut errors = Vec::new();
                let text = catalog.format_pattern(pattern, None, &mut errors);
                if errors.is_empty() {
                    return text.into_owned();
                }
            }
        }
        key.to_owned()
    }
}

fn bundle(
    locale: unic_langid::LanguageIdentifier,
    source: String,
) -> Result<FluentBundle<FluentResource>, String> {
    let resource = FluentResource::try_new(source)
        .map_err(|(_, errors)| format!("Invalid Fluent catalog: {errors:?}"))?;
    let mut bundle = FluentBundle::new(vec![locale]);
    bundle
        .add_resource(resource)
        .map_err(|errors| format!("Duplicate Fluent messages: {errors:?}"))?;
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_normalization_and_english_fallback() {
        assert_eq!(
            I18n::new(Some("pt_BR.UTF-8"), None).unwrap().text("unlock"),
            "Desbloquear"
        );
        assert_eq!(
            I18n::new(Some("zz-ZZ"), None).unwrap().text("unlock"),
            "Unlock"
        );
    }

    #[test]
    fn external_catalog_overrides_without_losing_builtins() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pt-BR.ftl"), "unlock = Entrar\n").unwrap();
        let i18n = I18n::new(Some("pt-BR"), Some(dir.path())).unwrap();
        assert_eq!(i18n.text("unlock"), "Entrar");
        assert_eq!(i18n.text("password"), "Senha");
        assert_eq!(i18n.text("missing-key"), "missing-key");
    }

    #[test]
    fn broken_external_catalog_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("en-US.ftl"), "unlock = {\n").unwrap();
        assert!(I18n::new(Some("en-US"), Some(dir.path())).is_err());
    }
}
