use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{DisplayFromStr, serde_as};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use url::Url;
use uuid::Uuid;

use crate::{
    HasHtmlUrl,
    integration_connection::IntegrationConnectionId,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
};

#[serde_as]
#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
pub struct WebPage {
    #[serde_as(as = "DisplayFromStr")]
    pub url: Url,
    pub title: String,
    pub timestamp: DateTime<Utc>,
    pub source: APISource,
    pub favicon: Option<Url>,
}

/// Maximum length, in characters, of a web page title sent by an API client.
pub const WEB_PAGE_TITLE_MAX_LENGTH: usize = 1024;

impl WebPage {
    /// Checks a web page sent by an API client: links must be `http(s)` URLs
    /// and the title must fit [`WEB_PAGE_TITLE_MAX_LENGTH`].
    pub fn validate(&self) -> Result<(), anyhow::Error> {
        let is_web_url = |url: &Url| matches!(url.scheme(), "http" | "https");
        if !is_web_url(&self.url) {
            return Err(anyhow!("Web page URL must be an http(s) URL"));
        }
        if self
            .favicon
            .as_ref()
            .is_some_and(|favicon| !is_web_url(favicon))
        {
            return Err(anyhow!("Web page favicon must be an http(s) URL"));
        }
        if self.title.chars().count() > WEB_PAGE_TITLE_MAX_LENGTH {
            return Err(anyhow!(
                "Web page title must be at most {WEB_PAGE_TITLE_MAX_LENGTH} characters"
            ));
        }
        Ok(())
    }
}

impl HasHtmlUrl for WebPage {
    fn get_html_url(&self) -> Url {
        self.url.clone()
    }
}

impl TryFrom<ThirdPartyItem> for WebPage {
    type Error = anyhow::Error;

    fn try_from(item: ThirdPartyItem) -> Result<Self, Self::Error> {
        match item.data {
            ThirdPartyItemData::WebPage(web_page) => Ok(*web_page),
            _ => Err(anyhow!(
                "Unable to convert ThirdPartyItem {} into WebPage",
                item.id
            )),
        }
    }
}

impl ThirdPartyItemFromSource for WebPage {
    fn into_third_party_item(
        self,
        user_id: UserId,
        integration_connection_id: IntegrationConnectionId,
    ) -> ThirdPartyItem {
        ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: self.source_id(),
            data: ThirdPartyItemData::WebPage(Box::new(self.clone())),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id,
            integration_connection_id,
            source_item: None,
        }
    }

    fn source_id(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.url.hash(&mut hasher);
        let url_hash = hasher.finish();
        format!("{:x}", url_hash)
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum APISource {
    UniversalInboxExtension,
    Other(String),
}

impl Serialize for APISource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            APISource::UniversalInboxExtension => {
                serializer.serialize_str("universalinboxextension")
            }
            APISource::Other(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for APISource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "universalinboxextension" => Ok(APISource::UniversalInboxExtension),
            _ => Ok(APISource::Other(s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    fn web_page(url: &str, title: &str, favicon: Option<&str>) -> WebPage {
        WebPage {
            url: url.parse().unwrap(),
            title: title.to_string(),
            timestamp: Utc::now(),
            source: APISource::UniversalInboxExtension,
            favicon: favicon.map(|favicon| favicon.parse().unwrap()),
        }
    }

    #[rstest]
    #[case::https(
        web_page("https://example.com", "title", Some("https://example.com/f.ico")),
        true
    )]
    #[case::http(web_page("http://example.com", "", None), true)]
    #[case::title_at_limit(
        web_page("https://example.com", &"é".repeat(WEB_PAGE_TITLE_MAX_LENGTH), None),
        true
    )]
    #[case::javascript_url(web_page("javascript:alert(1)", "title", None), false)]
    #[case::data_favicon(
        web_page("https://example.com", "title", Some("data:image/png;base64,AAAA")),
        false
    )]
    #[case::title_too_long(
        web_page("https://example.com", &"é".repeat(WEB_PAGE_TITLE_MAX_LENGTH + 1), None),
        false
    )]
    fn test_validate_web_page(#[case] page: WebPage, #[case] is_valid: bool) {
        assert_eq!(page.validate().is_ok(), is_valid);
    }
}
