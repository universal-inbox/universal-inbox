use anyhow::anyhow;
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{serde_as, DisplayFromStr};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use url::Url;
use uuid::Uuid;

use crate::{
    integration_connection::IntegrationConnectionId,
    third_party::item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemFromSource},
    user::UserId,
    HasHtmlUrl,
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
