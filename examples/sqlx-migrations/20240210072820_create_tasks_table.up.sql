-- Add up migration script here
CREATE TABLE tasks (
	id SERIAL PRIMARY KEY,
	task TEXT NOT NULL,
	completed BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX open_tasks_index ON tasks (id) WHERE completed is not true;

CREATE FUNCTION notify_task() RETURNS TRIGGER AS $notify_task$
	BEGIN
		PERFORM pg_notify('insert_tasks', NEW.id::text);
		RETURN NULL;
	END;
$notify_task$ LANGUAGE plpgsql;

CREATE TRIGGER tasks_insert_trigger
  AFTER INSERT ON tasks
  FOR EACH ROW EXECUTE FUNCTION notify_task();
