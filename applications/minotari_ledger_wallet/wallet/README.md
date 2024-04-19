# Instructions

To control ledger devices we use the ledgerctl library.
We install the supporting Python libraries from Ledger to control Ledger devices,
[LedgerCTL](https://github.com/LedgerHQ/ledgerctl). This we do with:
```
pip3 install --upgrade protobuf setuptools ecdsa
pip3 install git+https://github.com/LedgerHQ/ledgerctl
```

## Device configuration

See https://github.com/LedgerHQ/ledgerctl#device-configuration

Install a custom certificate on the device to help with development. Start the device in recovery mode (varies per device)
- Nano S Plus: Hold the left button while turning on, and follow on screen instructions
- Nano S: Hold the right button while turning on

Once in recovery mode run the following where <NAME> is simply the name of the CA. It can be anything:

```
ledgerctl install-ca <NAME>
```

## Building

Ledger does not easily compile locally and it is easiest  to compile via docker using their provided [ledger-app-builder](https://github.com/LedgerHQ/ledger-app-builder/).
See their readme for setup.
Once installed you can build the Tari Wallet for ledger by navigating to `./applications/minotari_ledger_wallet` and running the docker command:

```
docker run --rm -it -v ".:/app" ghcr.io/ledgerhq/ledger-app-builder/ledger-app-builder
```

This will load you into the docker vm where you can now build the ledger app.
where TARGET = nanosplus, nanos, etc.

```
cargo ledger build {TARGET}
```

Please note docker has no access to usb devices on MacOS. So the use of `cargo ledger build {TARGET} --load` will fail.

### Install `ledger`

- First delete the application if it was already installed

``` 
`ledgerctl delete "MinoTari Wallet"`
```

- Install with

```
`ledgerctl install app_nanosplus.json`
```
**Note:** In some cases the `cargo ledger build` action will invalidate `app_nanosplus.json` by setting the first line
to `"apiLevel": "0",` - ensure it is set to `"apiLevel": "1",`

### Running the ledger application

Start the `MinoTari Wallet` application on the Ledger by navigating to the app and pressing both buttons. You should
see `MinoTari Wallet` displayed on the screen. Now your device is ready to be used with the console wallet.

_**Note:** To manually exit the application, press both buttons on the Ledger._

**Errors**

- If the `MinoTari Wallet` application on the Ledger is not started when trying to access it with a desktop
  application, you should see the following error on the desktop:

  `Error: Ledger application not started`

- If the wrong application is started on the Ledger, you should see the following error on the desktop:

  `Error: Processing error 'Ledger application is not the MinoTari Wallet application: expected ...'`

- If the `MinoTari Wallet` application has an incorrect version, you should see the following error on the desktop:

  `Error: Processing error 'MinoTari Wallet application version mismatch: expected ...'`

## Emulator

Ledger has provided an in browser ledger emulator [Speculos](https://github.com/LedgerHQ/speculos)

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