# Instructions

## Ledger device check 

### Ledger Live

The Ledger device must be properly setup and accessible via Ledger Live. To verify:
- Connect your Ledger device to your computer and enter the passcode.
- Open Ledger Live, select `My Ledger` and ensure that the device information is displayed:
  - Device name and model
  - Firmware version
  - Genuine check

### Firmware

The minimum firmware version required for the Tari Ledger application 'Minotari Wallet' is checked each time the 
connected desktop application (example _Minotari Console Wallet_) is started. If not up to date, the desktop application
will not start and produce an error with a message to update the firmware.

If the firmware needs to be updated, it must be done via Ledger Live. To update the firmware, open Ledger Live, select
`My Ledger` and follow the instructions.

## Development environment setup

Ledger does not build with the standard library, so we need to install `rust-src`. This can be done with:
```
rustup component add rust-src --toolchain nightly
```

For loading a BOLOS application to a Ledger device, Ledger has actually written a command, called
[Cargo Ledger](https://github.com/LedgerHQ/cargo-ledger). This we need to install with:
```
cargo install --git https://github.com/LedgerHQ/cargo-ledger cargo-ledger
```

As per the [Cargo Ledger setup instructions](https://github.com/LedgerHQ/cargo-ledger#setup) run the following to add
new build targets for the current rust toolchain:

```
cargo ledger setup
```

## Device management via ledgerctl

To control ledger devices we use the `ledgerctl` Python application.

Ensure that Python 3 is installed on your machine. To test this, open a Python shell and run `pip3 --version`. 
Anaconda 3 is recommended. 

From a Python shell, install the required packages with the following commands:

```
pip3 install --upgrade protobuf setuptools ecdsa
pip3 install git+https://github.com/LedgerHQ/ledgerctl
```

Install a custom certificate on the device to help with development. Start the device in recovery mode (varies per 
device)
- Nano S Plus: Hold the left button while turning on, and follow on screen instructions
- Nano S: Hold the right button while turning on

Once in recovery mode run the following where <NAME> is simply the name of the CA. It can be anything:

```
ledgerctl install-ca <NAME>
```

For more information see [LedgerCTL](https://github.com/LedgerHQ/ledgerctl).

## Building

It is recommended to build the Ledger application via the official `ledger-app-builder` Docker image, as the Docker 
image is properly setup, supported and always kept up to date.

### Option 1: Using Docker

Ensure Docker Desktop is installed and running on your machine.

The following command has to be run from the root of the Tari repository.  

Replace ```{TARGET}``` with the appropriate value (e.g., `nanosplus`, `nanos`, etc.).

Compiled resources will be found in `applications/minotari_ledger_wallet/wallet/target/{TARGET}/release`

Single all-inclusive command without requiring manual interaction with the Docker container.

#### Unix/MacOS:
```bash
docker run --rm -it \
  -v ".:/app" \
  -w /app/applications/minotari_ledger_wallet/wallet \
  ghcr.io/ledgerhq/ledger-app-builder/ledger-app-builder \
  cargo ledger build {TARGET}
```

#### Windows:
```DOS
docker run --rm -it -v ".:/app" -w /app/applications/minotari_ledger_wallet/wallet ghcr.io/ledgerhq/ledger-app-builder/ledger-app-builder cargo ledger build {TARGET}
```

or  

If one would rather run smaller command snippets individually:   
* start with running a temporary docker container for building the ledger wallet:
```DOS
docker run --rm -it -v ".:/app" ghcr.io/ledgerhq/ledger-app-builder/ledger-app-builder
```
* change to the folder where the ledger wallet code can be found inside of the temporary docker container:
```DOS
cd /app/applications/minotari_ledger_wallet/wallet
```
* now build the ledger wallet then exit and close the temporary docker container:
```DOS
cargo ledger build {TARGET}
exit
```

**Notes:** 
- The application has to be installed on the device manually. Please see the _**Manual installation**_ section below.
- If any issues are encountered, please try to follow the instructions at 
  [ledger-app-builder](https://github.com/LedgerHQ/ledger-app-builder/).


### Option 2: Native build

It is possible to build the Ledger application natively, but this discouraged as too many issues have been found,
amongst others that the Ledger application might not execute correctly.

### Manual installation

This must be run from a Python 3 shell (`pip3 --version` should work).

If the application is running, first exit the application, otherwise these commands will fail.

- First delete the application if it was already installed

``` 
ledgerctl delete "MinoTari Wallet"
```

- Installation

First locate `app_nanosplus.json`. It will either be in the ledger wallet root
`/applications/minotari_ledger_wallet/wallet` or in its the target directory `./target/nanosplus/release`,
then run one of the following commands to install the application:

```
ledgerctl install app_nanosplus.json
```
```
ledgerctl install .\target\nanosplus\release\app_nanosplus.json
ledgerctl install .\target\stax\release\app_stax.json
```

**Notes for Windows users:**
- For a standard Anaconda 3 installation, the Python shell can be started from your development terminal with
  ```
  powershell -ExecutionPolicy ByPass -NoExit -Command "& 'C:\ProgramData\anaconda3\shell\condabin\conda-hook.ps1' ; conda activate 'C:\ProgramData\anaconda3' "
  ```

### Running the ledger application

Start the `MinoTari Wallet` application on the Ledger by navigating to the app and pressing both buttons. You should
see `MinoTari Wallet` displayed on the screen. Now your device is ready to be used with the console wallet.

_**Note:** To manually exit the application, press both buttons on the Ledger._

**Errors**

When trying to access the `MinoTari Wallet` Ledger application with a Tari desktop application, watch out for these 
errors:

- If the application is not started or if the wrong application is started, the following error is returned:

  `Ledger application is not the 'Minotari Wallet' application ...'`

- If the application has an incorrect version, the following error is returned:

  `'Minotari Wallet' application version mismatch ...'`

- If any processing error occurs with startup validation, one of the following errors is returned:

  - `'Minotari Wallet' application could not retrieve a public key ...`
  - `'Minotari Wallet' application could not create a signature ...`
  - `'Minotari Wallet' application could not create a valid signature ...`
  - `'Minotari Wallet' application is not creating unique signatures ...`

### Testing all functions on the ledger application

From the main Tari project root, run the following demo program. The Ledger application can be started or not started, 
as the demo program will prompt the user to perform the necessary actions.

```
cargo run --release --example ledger_demo
``` 

## Testing via the emulator

Using the emulator has been proved to not be accurate at all times, and it is recommended to rather test the 
Ledger application on a physical Ledger device.

For the Ledger browser ledger emulator see [Speculos](https://github.com/LedgerHQ/speculos)

To build on M1 devices clone the repository

```
$ git checkout df84117d2ac300cd277d58913a9f56e061b5fb2f

// Now build the docker image
$ docker build -t speculos-builder:latest -f build.Dockerfile .

// Now build the main docker image, which will be based of the builder image.

$ docker build -t speculos:latest .
```

Once built, run the emulator with:

```
docker run --rm -it -v $(pwd):/speculos/apps -p 1234:1234 -p 3000:3000 -p 40000:40000 -p 41000:41000 speculos --display headless --api-port 3000 --vnc-port 41000 apps/target/nanosplus/release/minotari_ledger_wallet --model nanosp
```

Browse to the address `http://localhost:3000`
