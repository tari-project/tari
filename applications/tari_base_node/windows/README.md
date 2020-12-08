# Tari Components Runtime Instructions

This `README` file, in the installation folder, contains important instructions 
before running `tari_base_node.exe`, `tari_console_wallet.exe` and 
`tari_merge_mining_proxy.exe` the first time.  

## Pre-requisites 

The `tari_base_node` executable has the following dependencies, which can be 
installed automatically if selected:
- SQLite
- OpenSSL
- Tor Services
- Redistributable for Microsoft Visual Studio 2019 
- XMRig

Notes: 
- Minimum Windows 7 64bit with Windows Powershell 3.0 required, 
  Windows 10 64bit recommended.
- Minimum 1.2 GB free disk space required at the initial runtime. 

## Automatic Installation

- Download the latest Windows installation file (and SHA-256 CRC) from 
  <https://tari.com/downloads/>.
- Run the installation file and select all the default options.
- Installation artefacts for a default installation will be:
  ```
  `%USERPROFILE%\.tari-testnet`
  |   LICENSE.md
  |   LICENSE.txt
  |   README.md
  |   README.txt
  |   start_tari_base_node.lnk
  |   start_tari_console_wallet.lnk
  |   start_tari_merge_mining_proxy.lnk
  |   start_xmrig.lnk
  |   start_tor.lnk
  |   unins000.dat
  |   unins000.exe
  |---config
  |       config.toml
  |---runtime
          get_openssl_win.ps1
          get_xmrig_win.ps1
          install_openssl.bat
          install_sqlite.bat
          install_tor_services.bat
          install_vs2019_redist.bat
          install_xmrig.bat
          source_base_node_env.bat
          source_console_wallet_env.bat
          source_merge_mining_proxy_env.bat
          source_xmrig_env.bat
          start_tari_base_node.bat
          start_tari_console_wallet.bat
          start_tari_merge_mining_proxy.bat
          start_tor.bat
          start_xmrig.bat
          tari_base_node.exe
          tari_console_wallet.exe
          tari_merge_mining_proxy.exe
  ```
  - The following environment variables are created with a default installation:
    - `TARI_TOR_SERVICES_DIR = %USERPROFILE%\.tor_services\Tor`
    - `TARI_SQLITE_DIR       = %USERPROFILE%\.sqlite`
    - `TARI_XMRIG_DIR        = %USERPROFILE%\.xmrig`

## Runtime

- Execute the `.\start_tari_base_node` shortcut; this will also start the Tor 
  services if not running already that needs to be running before the base node 
  can run (do not close the Tor console).
- Execute the `.\start_tari_console_wallet` shortcut; this will also start the 
  Tor services that needs to be running before the base node can run (do not 
  close the Tor console).
  
  **Note**: The Tor console will output `[notice] Bootstrapped 100% (done): Done` 
  when the Tor services have fully started.
  
- Execute the `.\start_tari_merge_mining_proxy` shortcut.
- Execute the `.\start_xmrig` shortcut.
- Runtime artefacts:
  - The blockchain will be created in the `.\ridcully` folder.
  - The wallet will be created in the `.\wallet` folfder.
  - All log files will be created in the `.\log\base_node`, `.\log\wallet` and 
    `.\log\proxy` folders.
  - The following configuration files will be created in the `.\config` folder if 
    the default runtime configuration `..\..\common\config\presets\config.toml` 
    was used:
    - `node_id.json`
    - `wallet_id.json`
    - `console_wallet_id.json`
    - `base_node_tor.json`
    - `wallet_tor.json`
    - `log4rs_base_node.yml`
    - `log4rs_console_wallet.yml`
    - `log4rs_merge_mining_proxy.yml`

## Start Fresh

- To delete log files, delete the `.\log` folder.
- To re-sync the blockchain, delete the `.\ridcully` folder.
- To destroy your wallet and loose all your hard-earned tXTR Tari coins, delete 
  the `.\wallet` folder.

## Manual Installation

### Pre-requisites 

- SQLite:
  - Download 64bit Precompiled Binaries for Windows for 
    [SQL Lite](https://www.sqlite.org/index.html). 
  - Extract to local path, e.g. `%USERPROFILE%\.sqlite`.
  - Add the path to the Tari environment variable, e.g. type
    ```
    setx TARI_SQLITE_DIR %USERPROFILE%\.sqlite
    setx /m USERNAME %USERNAME%
    ```
  
    in a command console.
    
    or
    
  - Ensure folder containing `sqlite3.dll`, is in the user or system path 
    environment variable (hint: type `path` in a command console to verify).

- OpenSSL:
  - Download full version of the 64bit Precompiled Binaries for Windows for
    [OpenSSL](https://slproweb.com/products/Win32OpenSSL.html)
  - Install using all the default prompts
  
    **Note**: It is important that the dlls are available in the path. To test:
    ```
    where libcrypto-1_1-x64.dll
    where libssl-1_1-x64.dll
    ``` 

- Tor Services
  - Donwload 
    [Tor Windows Expert Bundle](https://www.torproject.org/download/tor/).
  - Extract to local path, e.g. `%USERPROFILE%\.tor_services`.
  - Add the path to the Tari environment variable, e.g. type 
    ```
    setx TARI_TOR_SERVICES_DIR %USERPROFILE%\.tor_services
    setx /m USERNAME %USERNAME%
    ```
  
    in a command console.
    
    or
    
  - Ensure folder containing the Tor executable, `tor.exe`, is in the user 
    or system path environment variable (hint: type `path` in a command 
    console to verify).

- Microsoft Visual C++ 
  [Redistributable for Visual Studio 2019](https://support.microsoft.com/en-us/help/2977003/the-latest-supported-visual-c-downloads)
  - Download and install `x64: vc_redist.x64.exe`

- XMrig:
  [XMRig](https://xmrig.com/download)
  - Download 64bit Precompiled Binaries for Windows for XMreig. 
  - Extract all files to local path, e.g. `%USERPROFILE%\.xmrig`.
  - Add the path to the Tari environment variable, e.g. type
    ```
    setx TARI_XMRIG_DIR %USERPROFILE%\.xmrig
    setx /m USERNAME %USERNAME%
    ```
  
    in a command console.

### Build the Tari Base Node (and Integrated Wallet)

- Folder Structure
  - All references to folders from here on are relative to 
    `applications\tari_base_node\windows`, within the Tari project source code 
    folder structure.

- Tari Base Node Executable
  - Build `tari_base_node.exe` according to 
    [Building from source (Windows 10)](https://github.com/tari-project/tari#building-from-source-windows-10).
  - Copy `tari_base_node.exe` to `.`, `.\runtime` or other local path.
  - If not extracted to `.` or `.\runtime`, ensure the folder containing 
    `tari_base_node.exe` is in the path.

- Tari Base Node Runtime Configuration File
  - Copy  `..\..\common\config\presets\config.toml` to `.\config`

### Build the Tari Console Wallet

- Folder Structure
  - All references to folders from here on are relative to 
    `applications\tari_console_wallet\windows`, within the Tari project source 
    code folder structure.

- Tari Console Wallet Executable
  - Build `tari_console_wallet.exe` according to 
    [Building from source (Windows 10)](https://github.com/tari-project/tari#building-from-source-windows-10).
  - Copy `tari_console_wallet.exe` to `.`, `.\runtime` or other local path.
  - If not extracted to `.` or `.\runtime`, ensure the folder containing 
    `tari_console_wallet.exe` is in the path.

- Tari console Wallet Runtime Configuration File
  - Copy  `..\..\common\config\presets\config.toml` to `.\config`

### Build the Tari Merge Mining Proxy

- Folder Structure
  - All references to folders from here on are relative to 
    `applications\tari_merge_mining_proxy\windows`, within the Tari project 
    source code folder structure.

- Tari Merge Mining Proxy Executable
  - Build `tari_merge_mining_proxy.exe` according to 
    [Building from source (Windows 10)](https://github.com/tari-project/tari#building-from-source-windows-10).
  - Copy `tari_merge_mining_proxy.exe` to `.`, `.\runtime` or other local path.
  - If not extracted to `.` or `.\runtime`, ensure the folder containing 
    `tari_merge_mining_proxy.exe` is in the path.

- Tari Base Node Runtime Configuration File
  - Copy  `..\..\common\config\presets\config.toml` to `.\config`
 