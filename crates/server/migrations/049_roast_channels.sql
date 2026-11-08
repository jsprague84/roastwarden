-- Each roast keeps its channel list (docs/channels.md): the roaster's declared
-- channels as they were when the roast started recording (CHARGE), JSON as in
-- roaster_setup.channels; '[]' when none were declared. NULL: a roast from before, which reads
-- the roaster's current list.
ALTER TABLE roast_sessions ADD COLUMN channels TEXT;
