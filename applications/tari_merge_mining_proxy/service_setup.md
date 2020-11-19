# Tari Merge Mining Proxy - Windows Service

## Assumptions

The windows installer was used previously. The base_node and proxy have already been run once before with default options.

E.g.
```
tari_base_node --init --create_id
```

## Building the Service

From the root directory of the tari repository

```
cd applications
cd tari_merge_mining_proxy
cargo build --featues=winservice --release
cd windows_service
cd merge_mining_proxy_service_installer --featues=winservice --release
cd ..
cd merge_mining_proxy_service_uninstaller --featues=winservice --release
```

Then in the `target` directory copy the following executables:
```
tari_merge_mining_proxy_service.exe
merge_mining_proxy_service_installer.exe
merge_mining_proxy_service_uninstaller.exe
```
to `%userprofile%\.tari-testnet\runtime`

## Installing the Service

Run `merge_mining_proxy_service_installer.exe` as administrator.

Make sure the following settings are in `%userprofile%\config\windows.toml`:
```
...
[base_node.ridcully]
grpc_enabled = true
...
grpc_base_node_address = "127.0.0.1:18142"
...
grpc_console_wallet_address = "127.0.0.1:18143"
```


## Starting and stopping the service

Service can be started and stopped through the `Services` application.

Service can also be started from an elevated command line with:
```
net start tari_merge_mining_proxy_service
```

Service can also be stopped from an elevated command line with:
```
net stop tari_merge_mining_proxy_service
```

## Uninstalling the Service

Run `merge_mining_proxy_service_uninstaller.exe` as administrator.