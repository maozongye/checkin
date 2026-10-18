-- checkin_run / checkin_item — aligned with docs/03-schema.md
-- Idempotent unique key: (account_id, local_day, platform)

CREATE TABLE IF NOT EXISTS checkin_run (
  id           TEXT PRIMARY KEY,
  trigger      TEXT NOT NULL CHECK (trigger IN ('manual', 'schedule')),
  started_at   TEXT NOT NULL,
  finished_at  TEXT,
  status       TEXT NOT NULL CHECK (status IN ('running', 'done', 'partial', 'failed')),
  timezone     TEXT NOT NULL DEFAULT 'Asia/Shanghai',
  concurrency  INTEGER NOT NULL DEFAULT 3,
  note         TEXT
);

CREATE INDEX IF NOT EXISTS idx_checkin_run_started ON checkin_run(started_at DESC);

CREATE TABLE IF NOT EXISTS checkin_item (
  id             TEXT PRIMARY KEY,
  run_id         TEXT NOT NULL REFERENCES checkin_run(id) ON DELETE CASCADE,
  account_id     TEXT NOT NULL,
  platform       TEXT NOT NULL,
  local_day      TEXT NOT NULL,
  outcome        TEXT NOT NULL CHECK (outcome IN (
                   'Success', 'AlreadyDone', 'Skipped', 'Failed', 'Disabled'
                 )),
  credits_delta  INTEGER,
  streak_days    INTEGER,
  balance        INTEGER,
  message        TEXT,
  raw_code       INTEGER,
  retryable      INTEGER,
  started_at     TEXT NOT NULL,
  finished_at    TEXT,
  UNIQUE (account_id, local_day, platform)
);

CREATE INDEX IF NOT EXISTS idx_checkin_item_run ON checkin_item(run_id);
CREATE INDEX IF NOT EXISTS idx_checkin_item_day ON checkin_item(local_day, platform);
