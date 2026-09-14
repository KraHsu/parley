-- Optional public provenance only: never credentials or complete service configuration.
ALTER TABLE vocabulary_occurrences ADD COLUMN backend TEXT;
PRAGMA user_version = 6;
