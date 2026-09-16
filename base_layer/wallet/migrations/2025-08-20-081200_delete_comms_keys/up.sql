-- Drop comms key values - comms service has been removed

DELETE FROM client_key_values WHERE key = 'TorId';
