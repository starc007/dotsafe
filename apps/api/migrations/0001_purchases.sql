-- Who bought dotsafe, their license keys, and the devices those keys run on.
-- Filled from Dodo webhooks and the license routes. Dates are ISO 8601 UTC.

-- One row per payment (payment.succeeded), updated by refunds and disputes.
CREATE TABLE purchases (
  payment_id TEXT PRIMARY KEY,
  customer_id TEXT NOT NULL,
  email TEXT NOT NULL,
  name TEXT,
  -- In the currency's smallest unit (cents for USD), tax included.
  amount INTEGER NOT NULL,
  currency TEXT NOT NULL,
  paid_at TEXT NOT NULL,
  refunded_amount INTEGER,
  refunded_at TEXT,
  disputed_at TEXT
);
CREATE INDEX purchases_email ON purchases (email);

-- One row per license key (entitlement_grant.created). The key itself is kept
-- for support lookups; it is never logged.
CREATE TABLE licenses (
  id TEXT PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  payment_id TEXT,
  customer_id TEXT NOT NULL,
  email TEXT,
  activations_limit INTEGER,
  created_at TEXT NOT NULL,
  expires_at TEXT,
  -- When our license email went out; claimed before sending so it goes once.
  emailed_at TEXT,
  revoked_at TEXT,
  revoked_reason TEXT
);
CREATE INDEX licenses_email ON licenses (email);
CREATE INDEX licenses_payment ON licenses (payment_id);

-- One row per device activation (POST /v1/licenses/activate).
CREATE TABLE activations (
  instance_id TEXT PRIMARY KEY,
  license_id TEXT NOT NULL,
  device_name TEXT NOT NULL,
  activated_at TEXT NOT NULL,
  -- Last successful license check from that device.
  last_seen_at TEXT,
  deactivated_at TEXT
);
CREATE INDEX activations_license ON activations (license_id);

-- Webhooks already handled, so Dodo's retries don't apply one twice.
CREATE TABLE webhook_events (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  received_at TEXT NOT NULL
);
