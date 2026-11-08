-- A server restart after an Autotune 2 run stored the firmware's retained results message
-- again as a new run (the check compared it with the newest run of any kind; fixed in
-- tuning::ingest_results). Remove those copies: a firmware run whose results message is the
-- same as an earlier firmware run's on the same roaster, unless a gain change names it. The
-- firmware's message carries its own uptime stamp, so two real runs never share one.
DELETE FROM autotune_runs
WHERE source = 'firmware'
  AND EXISTS (
    SELECT 1 FROM autotune_runs o
    WHERE o.device_id = autotune_runs.device_id
      AND o.source = 'firmware'
      AND o.payload = autotune_runs.payload
      AND (o.created_at < autotune_runs.created_at
           OR (o.created_at = autotune_runs.created_at AND o.rowid < autotune_runs.rowid))
  )
  AND id NOT IN (SELECT run_id FROM pid_changes WHERE run_id IS NOT NULL);
