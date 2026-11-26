use std::error::Error;

#[allow(dead_code)]
pub type PlatformResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Открывает HTTP(S)-адрес в браузере по умолчанию.
#[allow(dead_code)]
pub fn open_url(url: &str) -> PlatformResult<()> {
    let url = url.trim();

    let parsed = url::Url::parse(url)?;

    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("Разрешены только HTTP и HTTPS".into()),
    }

    open::that_detached(parsed.as_str())?;

    Ok(())
}
