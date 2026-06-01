-- Backfill: slack-morphism 2.22 changed SlackRichTextList.elements from
-- Vec<SlackRichTextSection> (a bare struct serialized as {"elements":[...]} with no "type")
-- to Vec<SlackRichTextListElement> (#[serde(tag = "type")], requiring "type":"rich_text_section").
-- Data stored by <= 2.21 lacks that discriminator, so reads fail with `missing field type`.
-- This walks every third_party_item.data JSON tree and adds "type":"rich_text_section" to each
-- rich_text_list element that lacks a type. Idempotent. The third_party_item.data column is JSON
-- (not jsonb), so we cast in/out around the jsonb-typed helper.

CREATE OR REPLACE FUNCTION pg_temp.fix_rich_text_list(input jsonb) RETURNS jsonb AS $$
DECLARE
  result jsonb;
  k text;
  v jsonb;
  elem jsonb;
  fixed_elements jsonb;
BEGIN
  CASE jsonb_typeof(input)
  WHEN 'object' THEN
    result := '{}'::jsonb;
    FOR k, v IN SELECT * FROM jsonb_each(input) LOOP
      result := result || jsonb_build_object(k, pg_temp.fix_rich_text_list(v));
    END LOOP;
    IF result->>'type' = 'rich_text_list' AND jsonb_typeof(result->'elements') = 'array' THEN
      fixed_elements := '[]'::jsonb;
      FOR elem IN SELECT * FROM jsonb_array_elements(result->'elements') LOOP
        IF jsonb_typeof(elem) = 'object' AND NOT (elem ? 'type') THEN
          elem := jsonb_build_object('type', 'rich_text_section') || elem;
        END IF;
        fixed_elements := fixed_elements || jsonb_build_array(elem);
      END LOOP;
      result := jsonb_set(result, '{elements}', fixed_elements);
    END IF;
    RETURN result;
  WHEN 'array' THEN
    result := '[]'::jsonb;
    FOR elem IN SELECT * FROM jsonb_array_elements(input) LOOP
      result := result || jsonb_build_array(pg_temp.fix_rich_text_list(elem));
    END LOOP;
    RETURN result;
  ELSE
    RETURN input;
  END CASE;
END;
$$ LANGUAGE plpgsql;

UPDATE third_party_item
SET data = pg_temp.fix_rich_text_list(data::jsonb)::json
WHERE data::text LIKE '%"rich_text_list"%';
