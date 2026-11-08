-- Each roaster's declared channels (docs/channels.md): the extra values the
-- owner wants kept with a roast, drawn on the scope, usable in alarms and exported to Artisan.
-- JSON [{"key": "t3", "label": "Exhaust", "unit": "C", "max": null}, …]; NULL is none, so every
-- roaster keeps what it does today.
ALTER TABLE roaster_setup ADD COLUMN channels TEXT;
