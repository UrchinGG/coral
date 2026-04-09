CREATE TABLE starfish_hwid_components (
    id BIGSERIAL PRIMARY KEY,
    hwid_id BIGINT NOT NULL UNIQUE REFERENCES starfish_hwids(id) ON DELETE CASCADE,
    machine_guid_hash TEXT,
    smbios_uuid_hash TEXT,
    disk_serial_hash TEXT,
    cpu_id_hash TEXT,
    baseboard_serial_hash TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_starfish_hwid_components_hwid ON starfish_hwid_components(hwid_id);
