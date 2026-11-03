-- dotsafe issues its own license keys; the payment provider only takes the
-- payment. A key is stored as its SHA-256 hash plus its last four characters
-- (for support), so a leaked database exposes no working key. No Dodo-issued
-- keys were ever sold, so the old Dodo-shaped tables are replaced, not migrated.

DROP TABLE licenses;
DELETE FROM activations;

CREATE TABLE licenses (
  -- "lic_" and random characters.
  id TEXT PRIMARY KEY,
  key_hash TEXT NOT NULL UNIQUE,
  key_hint TEXT NOT NULL,
  email TEXT NOT NULL,
  name TEXT,
  -- "purchase" or "manual" (given away with license:create).
  source TEXT NOT NULL,
  -- The payment it came from, for purchases. One license per payment.
  payment_id TEXT UNIQUE,
  activations_limit INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  -- When the key was emailed; claimed before sending so it goes out once.
  emailed_at TEXT,
  revoked_at TEXT,
  revoked_reason TEXT
);
CREATE INDEX licenses_email ON licenses (email);
