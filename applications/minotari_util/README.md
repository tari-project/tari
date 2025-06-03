# Minotari Util

A command-line utility tool for Tari base node operations.

## Installation

Build from source:

```bash
cargo build --package minotari_util
```

## Usage

### Database Statistics

Analyze LMDB database usage for the base node:

```bash
# Basic usage with default table output
minotari_util dbstats

# Specify custom database path
minotari_util dbstats --db-path /path/to/tari/data/base_node/db

# JSON output format
minotari_util dbstats --format json

# CSV output format
minotari_util dbstats --format csv

# Show only top 10 databases by size
minotari_util dbstats --top 10

# Sort by number of entries
minotari_util dbstats --sort-by entries

# Export to file
minotari_util dbstats --export stats.json --format json
```

### Output Information

The `dbstats` command provides:

1. **Environment Information**
   - Total mapped size
   - Last page number
   - Transaction ID
   - Reader slots (max/used)

2. **Per-Database Statistics**
   - Database name
   - Number of entries
   - Total size (human-readable)
   - Average entry size
   - B-tree depth
   - Page statistics (leaf/branch/overflow)

3. **Summary**
   - Total databases
   - Total entries across all DBs
   - Total space used
   - Largest database
   - Average entries per database

### Options

- `--base-path <PATH>` - Override default data directory
- `--network <NETWORK>` - Specify network (mainnet/nextnet/stagenet/localnet)
- `--verbose/-v` - Verbose output
- `--help/-h` - Help information

## Future Commands

The tool is designed to be extensible for additional utilities:

- `validate` - Database integrity checks
- `backup` - Database backup utilities
- `export` - Export specific data sets
- `compact` - Database compaction tools
- `query` - Ad-hoc database queries
- `network-info` - Network status and peer information

## Examples

```bash
# Get database statistics for mainnet
minotari_util --network mainnet dbstats

# Analyze specific database path with custom output
minotari_util dbstats \
  --db-path /custom/path/db \
  --format table \
  --sort-by size \
  --top 15

# Export full stats to JSON for analysis
minotari_util dbstats \
  --format json \
  --export analysis.json \
  --include-detailed
```
