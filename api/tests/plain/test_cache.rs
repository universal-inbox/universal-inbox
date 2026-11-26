use std::time::Duration;

use cached::ConcurrentCachedAsync;
use pretty_assertions::assert_eq;
use slack_morphism::prelude::*;
use uuid::Uuid;

use universal_inbox_api::utils::cache::{Cache, build_redis_cache};

async fn with_test_namespace() {
    Cache::set_namespace(Uuid::new_v4().to_string()).await;
}

/// The `cached` crate's compact MessagePack encoding writes structs as positional
/// arrays: since slack-morphism 2.30, a `SlackBlock` read back from it silently decoded
/// as `SlackBlock::Unknown`. Cached values are JSON and must round-trip unchanged.
#[tokio::test]
async fn test_cached_slack_blocks_round_trip() {
    with_test_namespace().await;
    let cache =
        build_redis_cache::<Vec<SlackBlock>>("test:slack_blocks", Duration::from_secs(60), false)
            .await;
    let blocks: Vec<SlackBlock> = serde_json::from_value(serde_json::json!([
        {
            "type": "rich_text",
            "block_id": "1111",
            "elements": [
                {
                    "type": "rich_text_section",
                    "elements": [ { "type": "text", "text": "Hello", "style": { "bold": true } } ]
                }
            ]
        },
        { "type": "plan", "title": "Release plan", "tasks": [] },
        { "type": "some_future_block", "text": "Not modelled" }
    ]))
    .unwrap();

    cache
        .async_cache_set("key".to_string(), blocks.clone())
        .await
        .unwrap();
    let cached_blocks = cache
        .async_cache_get(&"key".to_string())
        .await
        .unwrap()
        .expect("the blocks must be cached");

    assert!(matches!(cached_blocks[0], SlackBlock::RichText(_)));
    assert!(matches!(cached_blocks[1], SlackBlock::Plan(_)));
    assert!(matches!(cached_blocks[2], SlackBlock::Unknown(_)));
    assert_eq!(cached_blocks, blocks);
}

#[tokio::test]
async fn test_undecodable_cached_value_is_a_cache_miss() {
    with_test_namespace().await;
    let prefix = "test:undecodable";
    // Same prefix and key, but a value of another type
    build_redis_cache::<String>(prefix, Duration::from_secs(60), false)
        .await
        .async_cache_set("key".to_string(), "not blocks".to_string())
        .await
        .unwrap();

    let cache = build_redis_cache::<Vec<SlackBlock>>(prefix, Duration::from_secs(60), false).await;

    assert_eq!(
        cache.async_cache_get(&"key".to_string()).await.unwrap(),
        None
    );
}
