CREATE TABLE stats (
  timestamp DATETIME NOT NULL,
  network VARCHAR(50) NOT NULL,
  service VARCHAR(50) NOT NULL,
  cpu REAL NOT NULL,
  memory REAL NOT NULL,
  upload REAL NOT NULL,
  download REAL NOT NULL,
  insertsPerTimestamp INTEGER DEFAULT 1,
  PRIMARY KEY(timestamp, network, service)
);
