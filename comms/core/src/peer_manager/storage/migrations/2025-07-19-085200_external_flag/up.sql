-- Table for MultiaddressesWithStats

ALTER TABLE multi_addresses
    ADD COLUMN is_external BOOLEAN NOT NULL DEFAULT TRUE;

UPDATE multi_addresses
SET is_external = CASE
    -- IPv4 loopback 127.0.0.0/8
    WHEN address LIKE '/ip4/127.%' THEN FALSE
    -- IPv4 unspecified 0.0.0.0
    WHEN address LIKE '/ip4/0.0.0.0%' THEN FALSE
    -- IPv4 private 10.0.0.0/8
    WHEN address LIKE '/ip4/10.%' THEN FALSE
    -- IPv4 private 172.16.0.0/12
    WHEN address LIKE '/ip4/172.16.%' THEN FALSE
    WHEN address LIKE '/ip4/172.17.%' THEN FALSE
    WHEN address LIKE '/ip4/172.18.%' THEN FALSE
    WHEN address LIKE '/ip4/172.19.%' THEN FALSE
    WHEN address LIKE '/ip4/172.20.%' THEN FALSE
    WHEN address LIKE '/ip4/172.21.%' THEN FALSE
    WHEN address LIKE '/ip4/172.22.%' THEN FALSE
    WHEN address LIKE '/ip4/172.23.%' THEN FALSE
    WHEN address LIKE '/ip4/172.24.%' THEN FALSE
    WHEN address LIKE '/ip4/172.25.%' THEN FALSE
    WHEN address LIKE '/ip4/172.26.%' THEN FALSE
    WHEN address LIKE '/ip4/172.27.%' THEN FALSE
    WHEN address LIKE '/ip4/172.28.%' THEN FALSE
    WHEN address LIKE '/ip4/172.29.%' THEN FALSE
    WHEN address LIKE '/ip4/172.30.%' THEN FALSE
    WHEN address LIKE '/ip4/172.31.%' THEN FALSE
    -- IPv4 private 192.168.0.0/16
    WHEN address LIKE '/ip4/192.168.%' THEN FALSE
    -- IPv6 loopback ::1
    WHEN address LIKE '/ip6/::1%' THEN FALSE
    -- IPv6 unspecified ::
    WHEN address LIKE '/ip6/::%' THEN FALSE
    -- All others external
    ELSE TRUE
END;
