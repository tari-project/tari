To run migrations:

```
diesel migration run --database-url temp.sqlite --config-file .\diesel.toml
```

To run global migrations
To run migrations:

```
diesel migration run --database-url temp-glob.sqlite --config-file .\diesel-global.toml --migration-dir ./global_db_migrations
```
