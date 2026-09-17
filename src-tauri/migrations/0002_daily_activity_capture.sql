ALTER TABLE work_logs ADD COLUMN origin VARCHAR(16) NOT NULL DEFAULT 'manual';
ALTER TABLE work_logs ADD COLUMN auto_capture_date DATE;
ALTER TABLE work_logs ADD COLUMN manually_edited BOOLEAN NOT NULL DEFAULT 0;
ALTER TABLE work_logs ADD COLUMN git_commit_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE work_logs ADD COLUMN agent_session_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE work_logs ADD COLUMN pending_evidence_count INTEGER NOT NULL DEFAULT 0;

CREATE UNIQUE INDEX IF NOT EXISTS ix_work_logs_auto_capture_date
  ON work_logs(auto_capture_date)
  WHERE auto_capture_date IS NOT NULL;

CREATE TABLE IF NOT EXISTS activity_sources (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_type VARCHAR(16) NOT NULL,
  path TEXT NOT NULL,
  display_name VARCHAR(160) NOT NULL,
  enabled BOOLEAN NOT NULL DEFAULT 1,
  discovered BOOLEAN NOT NULL DEFAULT 0,
  last_scanned_at DATETIME,
  last_error TEXT,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL,
  UNIQUE(source_type, path)
);

CREATE INDEX IF NOT EXISTS ix_activity_sources_enabled
  ON activity_sources(enabled, source_type);

CREATE TABLE IF NOT EXISTS daily_capture_settings (
  id INTEGER PRIMARY KEY CHECK(id = 1),
  enabled BOOLEAN NOT NULL DEFAULT 0,
  run_time TIME NOT NULL DEFAULT '18:00:00',
  timezone VARCHAR(64) NOT NULL DEFAULT 'Asia/Shanghai',
  lookback_days INTEGER NOT NULL DEFAULT 7,
  last_success_at DATETIME,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);

CREATE TABLE IF NOT EXISTS activity_evidence (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  activity_date DATE NOT NULL,
  source_id INTEGER NOT NULL REFERENCES activity_sources(id) ON DELETE CASCADE,
  source_type VARCHAR(16) NOT NULL,
  source_key TEXT NOT NULL UNIQUE,
  project VARCHAR(240) NOT NULL,
  summary TEXT NOT NULL,
  occurred_at DATETIME NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_activity_evidence_date
  ON activity_evidence(activity_date, occurred_at);

CREATE TABLE IF NOT EXISTS daily_capture_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  capture_date DATE NOT NULL UNIQUE,
  status VARCHAR(24) NOT NULL DEFAULT 'pending',
  work_log_id INTEGER REFERENCES work_logs(id) ON DELETE SET NULL,
  used_llm BOOLEAN NOT NULL DEFAULT 0,
  source_count INTEGER NOT NULL DEFAULT 0,
  failed_source_count INTEGER NOT NULL DEFAULT 0,
  git_commit_count INTEGER NOT NULL DEFAULT 0,
  agent_session_count INTEGER NOT NULL DEFAULT 0,
  pending_evidence_count INTEGER NOT NULL DEFAULT 0,
  message TEXT,
  started_at DATETIME NOT NULL,
  completed_at DATETIME,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);

INSERT OR IGNORE INTO daily_capture_settings(
  id, enabled, run_time, timezone, lookback_days, created_at, updated_at
) VALUES(
  1, 0, '18:00:00', 'Asia/Shanghai', 7,
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);
