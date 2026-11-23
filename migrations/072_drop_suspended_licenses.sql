UPDATE starfish_users SET license_status = 'inactive', updated_at = NOW() WHERE license_status = 'suspended';

ALTER TABLE starfish_users DROP CONSTRAINT starfish_users_license_status_check;
ALTER TABLE starfish_users ADD CONSTRAINT starfish_users_license_status_check
    CHECK (license_status IN ('inactive', 'active'));
