PRAGMA application_id = 1414810417;
PRAGMA user_version = 2;
ALTER TABLE tasks ADD COLUMN scope TEXT NOT NULL DEFAULT 'local_application' CHECK(scope = 'local_application');
