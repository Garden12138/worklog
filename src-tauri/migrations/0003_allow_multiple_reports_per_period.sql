DROP INDEX IF EXISTS ix_reports_type_period;
CREATE INDEX IF NOT EXISTS ix_reports_type_period ON reports(report_type, period_start, period_end);
