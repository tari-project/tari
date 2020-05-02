# Tari Base Node and Wallet Runtime Instructions

This `README` file, in the installation folder, contains important instructions 
before running `tari_base_node.exe` the first time.  

## Pre-requisites 

The `tari_base_node` executable has the following dependencies, which can be 
installed automatically if selected:
- SQLite
- Tor Services
- Redistributable for Microsoft Visual Studio 2019 

Notes: 
- Minimum Windows 7 64bit with Windows Powershell 3.0 required, 
  Windows 10 64bit recommended.
- Minimum 60GB free disk space required at the initial runtime. 

## Runtime

- Execute the `.\start_tari_basenode` shortcut; this will also start the Tor 
  services that needs to be running before the base node can run (do not close 
  the Tor console).
- The Tor console will output `[notice] Bootstrapped 100% (done): Done` 
  when the Tor services have fully started.
- Runtime artefacts:
  - The blockchain will be created in the `.\rincewind` folder.
  - The wallet will be created in the `.\wallet` folfder.
  - All log files will be created in the `.\log` folder.
  - The following configuration files will be created in the `.\config` folder if 
    the default runtime configuration `..\..\common\config\presets\windows.toml` 
    was used:
    - `wallet-identity.json`
    - `wallet-tor.json`
    - `node_id.json`
    - `tor.json`
    - `log4rs.yml`

## Start Fresh

- To delete log files, delete the `.\log` folder.
- To re-sync the blockchain, delete the `.\rincewind` folder.
- To destroy your wallet and loose all your hard-earned tXTR Tari coins, delete 
  the `.\wallet` folder.

# Installation Options

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
  |   start_tari_basenode.lnk
  |   start_tor.lnk
  |   unins000.dat
  |   unins000.exe
  |---config
  |       windows.toml
  |---runtime
          install_sqlite.bat
          install_tor_services.bat
          install_vs2019_redist.bat
          run_the_base_node.bat
          start_tari_basenode.bat
          start_tor.bat
          tari_base_node.exe
  ```
  - The following environment variables are created with a default installation:
    - `TARI_TOR_SERVICES_DIR = %USERPROFILE%\.tor_services\Tor`
    - `TARI_SQLITE_DIR       = %USERPROFILE%\.sqlite`

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
  
    in an Administrator command console.
    
    or
    
  - Ensure folder containing `sqlite3.dll`, is in the user or system path 
    environment variable (hint: type `path` in a command console to verify).

- Tor Services
  - Donwload 
    [Tor Windows Expert Bundle](https://www.torproject.org/download/tor/).
  - Extract to local path, e.g. `%USERPROFILE%\.tor_services`.
  - Add the path to the Tari environment variable, e.g. type 
    ```
    setx TARI_TOR_SERVICES_DIR %USERPROFILE%\.tor_services
    setx /m USERNAME %USERNAME%
    ```
  
    in an Administrator command console.
    
    or
    
  - Ensure folder containing the Tor executable, `tor.exe`, is in the user 
    or system path environment variable (hint: type `path` in a command 
    console to verify).

- Microsoft Visual C++ 
  [Redistributable for Visual Studio 2019](https://support.microsoft.com/en-us/help/2977003/the-latest-supported-visual-c-downloads)
  - Download and install `x64: vc_redist.x64.exe`

### Tari Base Node Runtime

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
  - Copy  `..\..\common\config\presets\windows.toml` to `.\config`
 