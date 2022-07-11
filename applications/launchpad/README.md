# Tari Launchpad - Tauri edition

a.k.a. _Tari one-click miner_.

## Prerequisites

1. Rust and cargo (https://www.rust-lang.org/tools/install).
2. NodeJs (v16.0 or higher) and Yarn (v 1.22 or higher).
3. Tauri CLI (`cargo install tauri-cli`). _Optional_.
4. [Docker](https://docs.docker.com/get-docker/) is not _strictly_ a pre-requisite, since the launchpad on-boarding 
   flow will install it for you, but you will need docker eventually, so putting it here.
5. Install the front-end dependencies
   ```text
   $ cd applications/launchpad  
   $ yarn
   $ cd gui-react
   $ yarn
   ```

## Running a development version of launchpad

These commands
* build the launchpad ReactJs front-end and launch it in development mode.
* build the backend in debug mode
* launch the application

```
$ cd applications/launchpad
$ yarn run tauri dev
```

### Debugging
The console relays debug messages from the launchpad backend (a Rust application).
The front-end is a standard ReactJs application wrapped inside a [Tauri](https://tauri.studio) desktop application. 
You can open a standard browser console in the front-end to debug front-end issues.


**Tip:** If you receive the following error 
`Unable to create base node...` there was a problem packaging the assets for the app.

## Building a production release
To build a production release, which also includes a bundled installer (.dmg on mac, .deb on linux, .msi on windows),
you can execute:

```
$ cd applications/launchpad
$ yarn run tauri build
```



## Viewing logs and configuration files

You can use the bundled Grafana environment that is packaged with launchpad to view log files. Or you can use your 
favorite text editor instead.

Logs and configuration files are stored in 
* MacOs: `~/Library/Caches/tari/tmp/dibbler/{app}/log`,
* Linux: `~/.cache/tari/tmp/dibbler/{app}/log`,
* Windows: `???`

You can edit the log configuration, `dibbler/config/log4rs.yml` to change the log level, output etc. Changes are 
picked up on the fly and take effect within 30s.

##  Miscellaneous notes

* The blockchain data is stores in docker volumes, and not on the host machine directly. This is due to crippling performance
limitations one suffers when mounting host file system from Windows or MacOS into docker containers.
This isn't a big drawback, since you seldom want or need to access the raw blockchain database files anyway. Are they're
[still accessible](#accessing-blockchain-data). But **ensure that you reserve enough space to store the Tari, and optionally,
Monero blockchains inside the Docker VM**.

### Accessing blockchain data

The blockchain data is stored in a docker volume for performance reasons. If you need to back up or access the LMDB
a blockchain data, you can use something like this to extract it to the host filesystem:

`docker run --rm -v $(pwd):/backup -v blockchain:/blockchain ubuntu tar czvf /backup/backup.tar.gz /blockchain`


## Layout

          +-----------------------+
          |                       |
    +---->|    Console Wallet     +------------------+
    |     |                       |                  |
    |     +----------+------------+                  |
    |                |                               |
    |                | gRPC                          |
    |                |                               |
    |                |                               |
    |     +----------v------------+           +------v-----+
    |     |                       |  Socks5   |            |
    |     |       Base Node       +---------->|     Tor    |----> Network
    |     |                       |           |            |
    |     +----------^------------+           +------------+
    |                |
    |                |
    |                |
    |     +----------+------------+
    |     |                       |
    +-----+      SHA3-Miner       |
    |     |                       |
    |     +-----------------------+
    |
    |
    |
    |     +-----------------------+
    |     |                       |
    +-----+        XMRRig etc     |
          |                       |
          +-----------------------+

## Building custom docker images

The docker images for the base node, wallet etc. are designed to handle the broadest set of chipsets and 
architectures. For this reason, they not be optimal for _your_ system. You can build custom images for launchpad 
using the `build_images.sh` script in this folder.

Refer to that script for further details and build options.

There are a set of files in this folder that offer a convenient way of setting the environment up for some common 
configurations.

run `source {env_config}.env` to set up the environment. Currently, the presets are:

* `local-performance-amd64.env`: For building local images with Intel/AMD and AVX-2 chipset support (about 2x 
  speedup on crypto operations)
* `local-performance-arm64.env`: For building local images for Apple M-series CPUs.
* `hosted-dual.env`: Replicates the CI enviroment. Builds safe multi-arch images and pushes them to the docker repo 
  (requires a write access token). 