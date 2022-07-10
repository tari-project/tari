
ALTER TABLE transactions RENAME TO tmp_transactions;
CREATE TABLE transactions (
  event VARCHAR(50) NOT NULL,
  id VARCHAR(50) NOT NULL,
  receivedAt DATETIME NOT NULL,
  status VARCHAR(50) NOT NULL,
  direction VARCHAR(50) NOT NULL,
  amount REAL NOT NULL,
  message VARCHAR(255) NOT NULL,
  source VARCHAR(255) NOT NULL,
  destination VARCHAR(255) NOT NULL,
  isCoinbase VARCHAR(50) DEFAULT 'false',
  network VARCHAR(50),
  PRIMARY KEY(id)
);
INSERT INTO transactions(event, id, receivedAt, status, direction, amount, message, "source", destination, isCoinbase, network)
  SELECT event, id, receivedAt, status, direction, amount, message, "source", destination, IIF(isCoinbase = 'true', 'true', 'false'), network FROM tmp_transactions;
DROP TABLE tmp_transactions;