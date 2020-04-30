# Tari Base Node and Wallet Runtime Instructions

This `README` file, in the installation folder, contains important instructions 
before running `tari_base_node.exe` the first time.  

## Pre-requisites 

- SQLite:
  - Download 32bit/64bit Precompiled Binaries for Windows for 
    [SQL Lite](https://www.sqlite.org/index.html). 
  - Extract to local path, e.g. `%USERPROFILE%\.sqlite`.
  - Ensure folder containing `sqlite3.dll`, e.g. `%USERPROFILE%\.sqlite`, is in 
    the user or system path environment variable (hint: type `path` in a command 
    console to verify).

- Tor
  - Donwload 
    [Tor Windows Expert Bundle](https://www.torproject.org/download/tor/).
  - Extract to local path, e.g. `C:\Program Files (x86)\Tor Services`.
  - Ensure folder containing the Tor executable, e.g. 
    `C:\Program Files (x86)\Tor Services\Tor`, is in the user or system path 
    environment variable (hint: type `path` in a command console to verify).

- Microsoft Visual C++ 
  [Redistributable for Visual Studio 2019](https://support.microsoft.com/en-us/help/2977003/the-latest-supported-visual-c-downloads)
  - Download and install `x86: vc_redist.x86.exe`, or
  - Download and install `x64: vc_redist.x64.exe`

## Runtime

- Execute the `.\start_tari_basenode` shortcut; this will also start the Tor 
  services that needs to be running before the base node can run (do not close 
  the Tor console).
- The Tor the console will output `[notice] Bootstrapped 100% (done): Done` 
  when the Tor services have fully started.
- Runtime artefacts:
  - The blockchain will be created in the `.\rincewind` folder.
  - The wallet will be created in the `.\wallet` folfder.
  - All log files will be created in the `.\log` folder.
  - The following configuration files will be created in the `.\config` folder if 
    runtime configuration `..\..\common\config\presets\windows.toml` was used:
    - `wallet-identity.json`
    - `wallet-tor.json`
    - `node_id.json`
    - `tor.json`
    - `log4rs.yml`

## Start Fresh

- To delete log files, delete the `.\log` folder.
- To re-sync the blockchain, delete the `.\rincewind` folder.1
- To destroy your wallet and loose all your hard-earned Tari coins, delete the 
  `.\wallet` folder.

# Installation Options

## Automatic Installation

- Download the latest Windows installation file (and SHA-256 CRC) from 
  <https://tari.com/downloads/>.
- Run the installation file.

## Manual Installation

- Folder Structure
  All references to folders from here on are relative to 
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
 