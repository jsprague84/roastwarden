-- First crack and dry end are counted from CHARGE, as the roast time is:
-- they were stored from the session start, a few seconds earlier or more. Correct the
-- stored times of roasts with a CHARGE mark.
UPDATE roast_sessions
SET first_crack_time = CAST(ROUND(
        (SELECT fc.elapsed_seconds FROM roast_events fc
         WHERE fc.session_id = roast_sessions.id AND fc.event_type = 'first_crack_start'
         ORDER BY fc.elapsed_seconds LIMIT 1)
      - (SELECT c.elapsed_seconds FROM roast_events c
         WHERE c.session_id = roast_sessions.id AND c.event_type = 'charge'
         ORDER BY c.elapsed_seconds LIMIT 1)) AS INTEGER)
WHERE first_crack_time IS NOT NULL
  AND EXISTS (SELECT 1 FROM roast_events c WHERE c.session_id = roast_sessions.id AND c.event_type = 'charge')
  AND EXISTS (SELECT 1 FROM roast_events f WHERE f.session_id = roast_sessions.id AND f.event_type = 'first_crack_start');

UPDATE roast_sessions
SET drying_end_time = CAST(ROUND(
        (SELECT d.elapsed_seconds FROM roast_events d
         WHERE d.session_id = roast_sessions.id AND d.event_type = 'drying_end'
         ORDER BY d.elapsed_seconds LIMIT 1)
      - (SELECT c.elapsed_seconds FROM roast_events c
         WHERE c.session_id = roast_sessions.id AND c.event_type = 'charge'
         ORDER BY c.elapsed_seconds LIMIT 1)) AS INTEGER)
WHERE drying_end_time IS NOT NULL
  AND EXISTS (SELECT 1 FROM roast_events c WHERE c.session_id = roast_sessions.id AND c.event_type = 'charge')
  AND EXISTS (SELECT 1 FROM roast_events d WHERE d.session_id = roast_sessions.id AND d.event_type = 'drying_end');
