-- No-op. This is a forward data normalization, not a schema change: it adds the
-- "type":"rich_text_section" discriminator that slack-morphism 2.22 requires on rich_text_list
-- elements. Reverting cannot distinguish backfilled rows from rows written natively by 2.22, so
-- stripping the field would corrupt valid data. Intentionally left as a no-op (mirrors the prior
-- irreversible data migration 20260413075607_remove_slack_star_support).
SELECT 1;
