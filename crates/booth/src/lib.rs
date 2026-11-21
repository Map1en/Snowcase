use std::time::Duration;

use serde::Deserialize;

const USER_AGENT: &str = "Snowcase";
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoothItem {
    pub id: u64,
    pub name: String,
    pub shop: String,
    pub url: String,
    pub image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub bytes: Vec<u8>,
    pub mime: String,
}

#[derive(Deserialize)]
struct Page {
    id: u64,
    name: String,
    url: String,
    shop: Shop,
    #[serde(default)]
    images: Vec<PageImage>,
}

#[derive(Deserialize)]
struct Shop {
    name: String,
}

#[derive(Deserialize)]
struct PageImage {
    original: String,
}

pub fn item_id(text: &str) -> Option<u64> {
    let text = text.trim();
    if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
        return text.parse().ok();
    }
    let host = text.split("://").nth(1).unwrap_or(text).split('/').next()?;
    if host != "booth.pm" && !host.ends_with(".booth.pm") {
        return None;
    }
    let (_, rest) = text.split_once("/items/")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

pub fn parse_item(json: &str) -> Result<BoothItem, String> {
    let page: Page = serde_json::from_str(json).map_err(|error| error.to_string())?;
    Ok(BoothItem {
        id: page.id,
        name: page.name,
        shop: page.shop.name,
        url: page.url,
        image: page.images.into_iter().next().map(|image| image.original),
    })
}

pub fn fetch(id: u64) -> Result<(BoothItem, Option<Image>), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(TIMEOUT)).build().into();
    let get = |url: &str| agent.get(url).header("User-Agent", USER_AGENT).call().map_err(|error| error.to_string());
    let json = get(&format!("https://booth.pm/ja/items/{id}.json"))?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let item = parse_item(&json)?;
    let picture = match &item.image {
        Some(url) if url.starts_with("https://booth.pximg.net/") => {
            let mut response = get(url)?;
            let mime = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("image/jpeg")
                .to_owned();
            let bytes = response.body_mut().read_to_vec().map_err(|error| error.to_string())?;
            Some(Image { bytes, mime })
        }
        _ => None,
    };
    Ok((item, picture))
}

#[cfg(test)]
mod tests;
