use std::{collections::HashMap, net::SocketAddr};

use config::{Config, Environment};
use pretty_assertions::assert_eq;
use rstest::*;

use universal_inbox_api::configuration::CimdSettings;

#[test]
fn test_unguarded_fetch_address_cannot_be_configured() {
    let environment = HashMap::from([
        (
            "UNIVERSAL_INBOX__OAUTH2__CIMD__UNGUARDED_FETCH_ADDRESS".to_string(),
            "127.0.0.1:8080".to_string(),
        ),
        (
            "UNIVERSAL_INBOX__OAUTH2__CIMD__TIMEOUT_SECS".to_string(),
            "9".to_string(),
        ),
    ]);
    let cimd_settings: CimdSettings = Config::builder()
        .add_source(
            Environment::with_prefix("universal_inbox")
                .separator("__")
                .source(Some(environment)),
        )
        .build()
        .unwrap()
        .get("oauth2.cimd")
        .unwrap();

    assert_eq!(
        (
            cimd_settings.timeout_secs,
            cimd_settings.unguarded_fetch_address,
            cimd_settings.unguarded_address()
        ),
        (9, None, None)
    );
}

#[rstest]
#[case::loopback_v4("127.0.0.1:8080", true)]
#[case::loopback_v6("[::1]:8080", true)]
#[case::private("10.0.0.1:80", false)]
#[case::loopback_mapped_in_v6("[::ffff:127.0.0.1]:8080", false)]
#[case::unspecified("0.0.0.0:8080", false)]
#[case::public("93.184.216.34:443", false)]
fn test_unguarded_address_is_honoured_for_loopback_only(
    #[case] address: SocketAddr,
    #[case] honoured: bool,
) {
    let cimd_settings = CimdSettings {
        unguarded_fetch_address: Some(address),
        ..CimdSettings::default()
    };

    assert_eq!(
        cimd_settings.unguarded_address(),
        honoured.then_some(address)
    );
}
