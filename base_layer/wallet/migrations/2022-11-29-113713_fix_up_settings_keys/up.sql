-- Your SQL goes here
UPDATE wallet_settings SET key = 'NodeFeatures' WHERE key = 'Nod features';
UPDATE wallet_settings SET key = 'ClientKey.recovery_data' WHERE key = 'ClientKey: "recovery_data"';
UPDATE wallet_settings SET key = 'ClientKey.console_wallet_custom_base_node_public_key' WHERE key = 'ClientKey: "console_wallet_custom_base_node_public_key"';
UPDATE wallet_settings SET key = 'ClientKey.console_wallet_custom_base_node_address' WHERE key = 'ClientKey: "console_wallet_custom_base_node_address"';
UPDATE wallet_settings SET key = 'BaseNodeChainMetadata' WHERE key = 'Last seen Chain metadata from basw node';
