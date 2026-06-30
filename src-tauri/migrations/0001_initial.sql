PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS work_logs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  work_date DATE NOT NULL,
  start_date DATE NOT NULL,
  end_date DATE NOT NULL,
  project VARCHAR(160) NOT NULL,
  task VARCHAR(240) NOT NULL,
  progress TEXT NOT NULL,
  result TEXT,
  blockers TEXT,
  hours REAL,
  priority VARCHAR(32) NOT NULL DEFAULT 'medium',
  notes TEXT,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_work_logs_work_date ON work_logs(work_date);
CREATE INDEX IF NOT EXISTS ix_work_logs_start_date ON work_logs(start_date);
CREATE INDEX IF NOT EXISTS ix_work_logs_end_date ON work_logs(end_date);
CREATE INDEX IF NOT EXISTS ix_work_logs_project ON work_logs(project);

CREATE TABLE IF NOT EXISTS templates (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name VARCHAR(160) NOT NULL,
  template_type VARCHAR(48) NOT NULL,
  content TEXT NOT NULL,
  is_default BOOLEAN NOT NULL DEFAULT 0,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_templates_template_type ON templates(template_type);

CREATE TABLE IF NOT EXISTS llm_settings (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider VARCHAR(48) NOT NULL,
  base_url VARCHAR(500) NOT NULL,
  model VARCHAR(160) NOT NULL,
  api_key TEXT,
  extra_headers TEXT,
  timeout_seconds INTEGER NOT NULL DEFAULT 60,
  is_active BOOLEAN NOT NULL DEFAULT 1,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);

CREATE TABLE IF NOT EXISTS email_settings (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  host VARCHAR(255) NOT NULL,
  port INTEGER NOT NULL,
  security VARCHAR(32) NOT NULL DEFAULT 'starttls',
  username VARCHAR(320) NOT NULL,
  password TEXT NOT NULL DEFAULT '',
  sender_address VARCHAR(320) NOT NULL,
  sender_name VARCHAR(160),
  is_active BOOLEAN NOT NULL DEFAULT 1,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);

CREATE TABLE IF NOT EXISTS recipients (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name VARCHAR(160) NOT NULL,
  email VARCHAR(320) NOT NULL UNIQUE,
  is_default BOOLEAN NOT NULL DEFAULT 0,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS ix_recipients_email ON recipients(email);

CREATE TABLE IF NOT EXISTS report_schedules (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  report_type VARCHAR(48) NOT NULL UNIQUE,
  enabled BOOLEAN NOT NULL DEFAULT 1,
  weekday VARCHAR(3),
  day_of_month INTEGER,
  template_id INTEGER REFERENCES templates(id) ON DELETE SET NULL,
  run_time TIME NOT NULL,
  auto_send BOOLEAN NOT NULL DEFAULT 0,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_report_schedules_report_type ON report_schedules(report_type);

CREATE TABLE IF NOT EXISTS report_schedule_recipients (
  schedule_id INTEGER NOT NULL REFERENCES report_schedules(id) ON DELETE CASCADE,
  recipient_id INTEGER NOT NULL REFERENCES recipients(id) ON DELETE RESTRICT,
  PRIMARY KEY(schedule_id, recipient_id)
);

CREATE TABLE IF NOT EXISTS reports (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  report_type VARCHAR(48) NOT NULL,
  title VARCHAR(240) NOT NULL,
  period_start DATE NOT NULL,
  period_end DATE NOT NULL,
  template_id INTEGER REFERENCES templates(id) ON DELETE SET NULL,
  content_markdown TEXT NOT NULL,
  status VARCHAR(32) NOT NULL DEFAULT 'draft',
  source_log_ids TEXT NOT NULL DEFAULT '[]',
  generated_at DATETIME,
  edited_at DATETIME,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS ix_reports_type_period ON reports(report_type, period_start, period_end);

CREATE TABLE IF NOT EXISTS report_email_deliveries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  report_id INTEGER NOT NULL REFERENCES reports(id) ON DELETE CASCADE,
  subject VARCHAR(240) NOT NULL,
  recipients_json TEXT NOT NULL,
  content_markdown TEXT NOT NULL,
  status VARCHAR(32) NOT NULL DEFAULT 'pending',
  error_message TEXT,
  sent_at DATETIME,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_report_email_deliveries_report_id ON report_email_deliveries(report_id);

CREATE TABLE IF NOT EXISTS generation_tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  report_type VARCHAR(48) NOT NULL,
  period_start DATE NOT NULL,
  period_end DATE NOT NULL,
  status VARCHAR(32) NOT NULL DEFAULT 'pending',
  message TEXT,
  report_id INTEGER REFERENCES reports(id) ON DELETE SET NULL,
  completed_at DATETIME,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);

CREATE TABLE IF NOT EXISTS desktop_preferences (
  id INTEGER PRIMARY KEY CHECK(id = 1),
  launch_at_login BOOLEAN NOT NULL DEFAULT 0,
  legacy_database_path TEXT,
  migrated_at DATETIME,
  created_at DATETIME NOT NULL,
  updated_at DATETIME NOT NULL
);
