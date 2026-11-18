-- Migration: 0004_xcli_tasks.sql
ALTER TABLE generation_tasks ADD COLUMN xcli_task_id TEXT;
ALTER TABLE generation_tasks ADD COLUMN last_seq INTEGER NOT NULL DEFAULT 0;
ALTER TABLE generation_tasks ADD COLUMN step_progress TEXT;
ALTER TABLE generation_tasks ADD COLUMN prompt_inputs TEXT;
ALTER TABLE generation_tasks ADD COLUMN template_id INTEGER;
ALTER TABLE generation_tasks ADD COLUMN title TEXT;
ALTER TABLE generation_tasks ADD COLUMN overwrite BOOLEAN NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS ix_generation_tasks_xcli_task_id ON generation_tasks(xcli_task_id);

