-- A touch panel's token (docs/panel.md): the one roaster it was made for. It may use that
-- roaster's panel and nothing else, and needs no "tokens may control this roaster" switch for
-- it. NULL for every other token.
ALTER TABLE api_tokens ADD COLUMN roaster TEXT;
