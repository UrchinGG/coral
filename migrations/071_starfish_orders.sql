CREATE TABLE IF NOT EXISTS starfish_orders (
    order_id TEXT PRIMARY KEY,
    discord_id BIGINT NOT NULL,
    total_amount BIGINT NOT NULL,
    currency TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'paid',
    chargeback_sequence BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_starfish_orders_discord_id ON starfish_orders (discord_id);
