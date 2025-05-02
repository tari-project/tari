# Basic build environment setup guide for Windows

This basic guide uses much information sourced from https://github.com/KhronosGroup/OpenCL-Guide/blob/main/chapters/getting_started_windows.md. Information needed for developing ```Tauri apps``` was sourced from https://v1.tauri.app/v1/guides/getting-started/prerequisites

> This guide assumes that you are installing most of these components for the first time (excluding ```App Installer```, which is normally packaged with Windows.).

This guide utilises the PowerShell command line to install the dependencies.

You will require the following package managers to set up the development environment:
* winget
* Choclately
* vcpkg

The following dependencies are required, and will be installed as you proceed through this guide:
* OpenSSL
* Visual Studio BuildTools 2022
* CMake
* Protocol buffers (otherwise known as protobuf)
* SQLite3
* (Optional) Tor. While not required to build the project, Tari does leverage Tor for various functions.

> Note regarding UAC: Occasionally, you will require UAC related privileges in order perform specific actions. This may cause some actions to fail silently due to a lack of UAC credentials being provided. Please refer to the following [link](https://support.microsoft.com/en-us/windows/user-account-control-settings-d5b2046b-dcb8-54eb-f732-059f321afe18#:~:text=You%20can%20change%20the%20UAC,OK%20to%20save%20your%20changes) for more information about UAC. The default setting should be acceptable.

## Setting up ```winget``` and ```App Installer```

Users will require ```winget```, a Windows package manager bundled with ```App Installer```. This guide will require ```App Installer``` to be at the latest version.

You likely already have them installed. Run the following command in Powershell with Administrator privileges to confirm if they have been installed (to run PowerShell as Administrator, open the Start Menu, search for PowerShell, then right-click on the result and select the "Run as Administrator"):

```powershell
winget list
```
The expected output would be something along the lines of:

```powershell
PS C:\Users\leet> winget list
Name                                    Id                                       Version          Available      Source
-----------------------------------------------------------------------------------------------------------------------
Microsoft Visual Studio Installer       ARP\Machine\X64\{6F320B93-EE3C-4826-85E… 3.11.2180.21897
Tari Universe (Beta)                    ARP\Machine\X64\{A2500DE1-1C20-4E7E-9C5… 0.5.60.41105
Visual Studio Build Tools 2022          Microsoft.VisualStudio.2022.BuildTools   17.11.5                         winget
Microsoft Edge                          Microsoft.Edge                           130.0.2849.68                   winget
Microsoft Edge Update                   ARP\Machine\X86\Microsoft Edge Update    1.3.195.31
Microsoft Edge WebView2 Runtime         Microsoft.EdgeWebView2Runtime            130.0.2849.56                   winget
Microsoft Visual C++ 2015-2022 Redistr… Microsoft.VCRedist.2015+.x64             14.40.33810.0    14.40.33816.0  winget
Microsoft OneDrive                      Microsoft.OneDrive                       24.201.1006.0005                winget
Clipchamp                               MSIX\Clipchamp.Clipchamp_2.2.8.0_neutra… 2.2.8.0
```

If you do not see the above, it can be the result of several issues, listed below:
* **User has not run winget before**: You will be required to accept the terms and conditions that govern the use of ```winget``` and its sources. Read and accept the terms, which will then proceed to list the available applications.
* **Receive an error message "Failed when searching source; results will not be included: winget" or "Failed in attempting to update the source: winget"**: ```winget``` is installed, but ```App Installer``` is not at the latest version. To update ```App Installer```, you will need to run ```winget upgrade --id Microsoft.DesktopAppInstaller```
* **Terminal displays a blank result**: this means that ```App Installer```, and by extension ```winget```, is not installed. You will need to manually install it via the Microsoft Store. Use Microsoft Edge, and open the following URL in the browser: https://www.microsoft.com/p/app-installer/9nblggh4nns1#activetab=pivot:overviewtab, then click the install button to install it. It is best to restart the machine following the installation.

Then we can start installing components that will be needed to compile the ```Tari protocol tools``` locally.

## Install Visual Studio BuildTools 2022
To install, run the following command:
```Powershell
winget install --id=Microsoft.VisualStudio.2022.BuildTools  -e
```
Sample output would look something like:

```powershell
PS C:\Users\leet> winget install "Visual Studio BuildTools 2022 installer"
Found Visual Studio BuildTools 2022 [Microsoft.VisualStudio.2022.BuildTools] Version 17.11.5
This application is licensed to you by its owner.
Microsoft is not responsible for, nor does it grant any licenses to, third-party packages.
Downloading https://download.visualstudio.microsoft.com/download/pr/69e24482-3b48-44d3-af65-51f866a08313/471c9a89fa8ba27d356748ae0cf25eb1f362184992dc0bb6e9ccf10178c43c27/vs_BuildTools.exe
  ██████████████████████████████  4.22 MB / 4.22 MB
Successfully verified installer hash
Starting package install...
Successfully installed
```

This will save a ```setup.exe``` file to ```C:\Program Files (x86)\Microsoft Visual Studio\Installer\```, which will be used to install the various required components in the next step.

## Install Visual Studio components for Windows 11

In this step, we will be installing several components required by Visual Studio for the build

- ### 🚨 Ensure that all Visual Studio applications are closed and not running when performing the below command.

To install, run the following command: 

```powershell
& "C:\Program Files (x86)\Microsoft Visual Studio\Installer\setup.exe" install --force --focusedUi --productId Microsoft.VisualStudio.Product.BuildTools --channelId VisualStudio.17.Release --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.VC.Redist.14.Latest --add Microsoft.VisualStudio.Component.Windows11SDK.26100 --add Microsoft.VisualStudio.Component.VC.CMake.Project --add Microsoft.VisualStudio.Component.VC.CoreBuildTools --add Microsoft.VisualStudio.Component.VC.CoreIde --add Microsoft.VisualStudio.ComponentGroup.NativeDesktop.Core
```
A sample of the beginning of the expected output:
```powershell
PS C:\Users\leet> & "C:\Program Files (x86)\Microsoft Visual Studio\Installer\setup.exe" install --norestart --productId Microsoft.VisualStudio.Product.BuildTools --channelId VisualStudio.17.Release --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.VC.Redist.14.Latest --add Microsoft.VisualStudio.Component.Windows11SDK.22000
PS C:\Users\leet> [1d44:0001][2024-11-05T02:37:56] Saving the current locale (en-US) to user.json.
[1d44:0001][2024-11-05T02:37:56] Setting the telemetry services
[1d44:0005][2024-11-05T02:37:56] Creating a new telemetry service.
[1d44:0001][2024-11-05T02:37:56] Visual Studio Installer Version: 3.11.2180
[1d44:0001][2024-11-05T02:37:56] Raw Command line: "C:\Program Files (x86)\Microsoft Visual Studio\Installer\setup.exe" install --passive --norestart --productId Microsoft.VisualStudio.Product.BuildTools --channelId VisualStudio.17.Release --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.VC.Redist.14.Latest --add Microsoft.VisualStudio.Component.Windows11SDK.22000
[1d44:0001][2024-11-05T02:37:56] Parsed command line options: install --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 Microsoft.VisualStudio.Component.VC.Redist.14.Latest Microsoft.VisualStudio.Component.Windows11SDK.22000 --channelId VisualStudio.17.Release --norestart --passive --productId Microsoft.VisualStudio.Product.BuildTools
[1d44:0005][2024-11-05T02:37:56] Telemetry session ID: 8c0666e6-122f-43a2-8400-3c9a47d5d8d1
[1d44:0004][2024-11-05T02:37:56] Creating new ExperimentationService
```
Visual Studio Installer should download and install components requested.

### Dealing with errors during installation of Visual Studio Components
If the installer fails, you'll often see error codes or failure messages in the PowerShell output. Look for lines like:
     ```PowerShell
     Install failed with exit code ...
     Component ... could not be installed
     ```

Logs are saved under `C:\ProgramData\Microsoft\VisualStudio\Packages\_Instances\` or `C:\Users\<YourUser>\AppData\Local\Temp\dd_installer_*`

3. **Exit codes** (common ones):
   - `0`: Success
   - `1603`: Fatal install error (common if prerequisites fail)
   - `3010`: Success but requires reboot

If you have failed to install the components, you can manually launch the **Visual Studio Installer**. You’ll see all installed versions of Visual Studio. Click **Modify** next to **Visual Studio 2022**.

Switch to the **Individual Components** tab (at the top). Use the search bar or scroll to find and check the boxes for specific components.

After selecting everything you need, click **Modify** (bottom right). Wait for the installer to download and install the new components. Restart Visual Studio after the process completes.

## Install ```git``` 

You will require ```git``` installed. To install, run the following command:

```powershell
winget install --id Git.Git -e --source winget
```
A sample of the expected output:
```powershell
PS C:\Users\leet> winget install --id Git.Git -e --source winget
>>
Found Git [Git.Git] Version 2.47.0.2
This application is licensed to you by its owner.
Microsoft is not responsible for, nor does it grant any licenses to, third-party packages.
Downloading https://github.com/git-for-windows/git/releases/download/v2.47.0.windows.2/Git-2.47.0.2-64-bit.exe
  ██████████████████████████████  65.5 MB / 65.5 MB
Successfully verified installer hash
Starting package install...
Successfully installed
```

## Install ```chocolatey``` package manager 
```chocolatey``` is a Window's package manager that draws from a different set of repos than ```winget```, but will make the process of installing further required dependencies and packages such as ```protobuf``` easier.

To install, , run the following command:

```PowerShell
winget install --id chocolatey.chocolatey
```
sample output:

```Powershell
PS C:\Users\leet> winget install --id chocolatey.chocolatey
Found Chocolatey [Chocolatey.Chocolatey] Version 2.3.0.0
This application is licensed to you by its owner.
Microsoft is not responsible for, nor does it grant any licenses to, third-party packages.
Downloading https://github.com/chocolatey/choco/releases/download/2.3.0/chocolatey-2.3.0.0.msi
  ██████████████████████████████  6.03 MB / 6.03 MB
Successfully verified installer hash
Starting package install...
Successfully installed
Notes: The Chocolatey CLI MSI is intended for installation only! If upgrading from 5.x of Licensed Extension, or 1.x of other Chocolatey products, see the upgrade guide at https://ch0.co/upv2v6 before continuing. Otherwise, run `choco upgrade chocolatey`.
```
> Note: It is required to close all PowerShell terminals once this is complete. Failure to do so will result in any ```choco``` commands not being interpreted correctly until PowerShell has been restarted.

## Install Protobuf with chocolatey

Using a new PowerShell console (note that PowerShell needs be run as Administrator, otherwise protobuf will not install), run the following command:

```PowerShell
choco upgrade protoc -y
```

This will attempt to upgrade an existing ```protobuf``` install. If not installed, the command will then install ```protobuf```, then upgrade it.

sample output:
```
PS C:\Users\leet> choco upgrade protoc -y
Chocolatey v2.3.0
Upgrading the following packages:
protoc
By upgrading, you accept licenses for the packages.
protoc is not installed. Installing...
Downloading package from source 'https://community.chocolatey.org/api/v2/'
Progress: Downloading chocolatey-compatibility.extension 1.0.0... 100%

[...]

protoc v28.3.0 [Approved]
protoc package files upgrade completed. Performing other installation steps.
Extracting 64-bit C:\ProgramData\chocolatey\lib\protoc\tools\protoc-28.3-win64.zip to C:\ProgramData\chocolatey\lib\protoc\tools...
C:\ProgramData\chocolatey\lib\protoc\tools
 ShimGen has successfully created a shim for protoc.exe
 The upgrade of protoc was successful.
  Deployed to 'C:\ProgramData\chocolatey\lib\protoc\tools'

Chocolatey upgraded 3/3 packages.
 See the log for details (C:\ProgramData\chocolatey\logs\chocolatey.log).
```

## Install Windows vcpkg package manager

The last package manager you'll need to install is ```vcpkg```. This will be used for the installation of ```OpenSSL```.

To install ```vcpkg```, run the following commands:

```PowerShell
git clone https://github.com/microsoft/vcpkg.git \vcpkg
cd \vcpkg
.\bootstrap-vcpkg.bat
vcpkg integrate install
```

Below is a sample of the successful execution of the commands above:

sample output:
```powershell
PS C:\Users\leet> git clone https://github.com/microsoft/vcpkg.git \vcpkg\
>>
Cloning into '\vcpkg'...
remote: Enumerating objects: 254680, done.
remote: Counting objects: 100% (17338/17338), done.
remote: Compressing objects: 100% (686/686), done.
remote: Total 254680 (delta 17021), reused 16753 (delta 16652), pack-reused 237342 (from 1)
Receiving objects: 100% (254680/254680), 78.44 MiB | 6.35 MiB/s, done.
Resolving deltas: 100% (168180/168180), done.
Updating files: 100% (11832/11832), done.
PS C:\Users\leet> cd \vcpkg\
PS C:\vcpkg> .\bootstrap-vcpkg.bat
Downloading https://github.com/microsoft/vcpkg-tool/releases/download/2024-11-12/vcpkg.exe -> C:\vcpkg\vcpkg.exe... done.
Validating signature... done.

vcpkg package management program version 2024-11-12-eb492805e92a2c14a230f5c3deb3e89f6771c321
```

To confirm `vcpkg` is installed and working, run the following command:
```bash
vcpkg list
```

A successful message should show the following:
```bash
PS C:\Users\leet> vcpkg list
vcpkg-cmake-config:x64-windows                    2024-05-23
vcpkg-cmake-get-vars:x64-windows                  2024-09-22
vcpkg-cmake:x64-windows                           2024-04-23
```

If you get an error like the one below:

```bash
vcpkg : The term 'vcpkg' is not recognized as the name of a cmdlet, function, script file, or operable program. Check
the spelling of the name, or if a path was included, verify that the path is correct and try again.
At line:1 char:1
+ vcpkg list
+ ~~~~~
    + CategoryInfo          : ObjectNotFound: (vcpkg:String) [], CommandNotFoundException
    + FullyQualifiedErrorId : CommandNotFoundException
```
Then `vcpkg` has not been added to your path. Run the following command (you will require a PowerShell terminal running as administrator for this):

```bash
setx /m PATH "$Env:Path;C:\vcpkg"
```

Then either restart the PowerShell or open a new PowerShell with administrative rights, then run `vcpkg list` to confirm the environment path has been properly set.



## Install `SQLite3` with vcpkg
```powershell
vcpkg install sqlite3:x64-windows-static
```

Sample output:
```powershell
PS C:\Users\leet> vcpkg install sqlite3:x64-windows-static
Computing installation plan...
...
sqlite3:x64-windows-static package ABI: <hash>
Total install time: <time>
```

## Install ```OpenSSL``` with vcpkg
To install ```OpenSSL```, run the following commands:

```powershell
$Env:Path += ';C:\vcpkg'
vcpkg install openssl:x64-windows-static
```

Below is a sample output (with many of the intervening steps omitted via the "[...]") of a successful run of the above command:

```powershell
PS C:\Users\leet> $Env:Path += ';C:\vcpkg'
>>
PS C:\Users\leet> vcpkg install openssl:x64-windows-static
>>
Computing installation plan...
Computing installation plan...
A suitable version of cmake was not found (required v3.30.1).
Downloading cmake-3.30.1-windows-i386.zip
Successfully downloaded cmake-3.30.1-windows-i386.zip.
Extracting cmake...

[...]

Successfully downloaded msys2-msys2-runtime-3.5.4-2-x86_64.pkg.tar.zst.
-- Using msys root at C:/vcpkg/downloads/tools/msys2/21caed2f81ec917b
-- Fixing pkgconfig file: C:/vcpkg/packages/openssl_x64-windows-static/debug/lib/pkgconfig/libcrypto.pc
-- Fixing pkgconfig file: C:/vcpkg/packages/openssl_x64-windows-static/debug/lib/pkgconfig/libssl.pc
-- Fixing pkgconfig file: C:/vcpkg/packages/openssl_x64-windows-static/debug/lib/pkgconfig/openssl.pc
-- Installing: C:/vcpkg/packages/openssl_x64-windows-static/share/openssl/usage
-- Installing: C:/vcpkg/packages/openssl_x64-windows-static/share/openssl/copyright
-- Performing post-build validation
Stored binaries in 1 destinations in 13 s.
Elapsed time to handle openssl:x64-windows-static: 12 min
openssl:x64-windows-static package ABI: 746f9866315ce83ce1152f628b0dc320c6c36af665378d4a042c3385da77ce43
Total install time: 12 min
openssl is compatible with built-in CMake targets:

  find_package(OpenSSL REQUIRED)
  target_link_libraries(main PRIVATE OpenSSL::SSL)
  target_link_libraries(main PRIVATE OpenSSL::Crypto)
```

Once installed, you'll need to also set the OpenSSL environmental path. Use the following command to set an system-wide environmental path for OpenSSL:

```powershell
setx /m PATH "C:\vcpkg\installed\x64-windows-static\bin;$Env:Path"
```

Now verify that the installation has been done correctly with the following command:

```PowerShell
echo $env:OPENSSL_CONF $env:OPENSSL_DIR  
```

- C:\vcpkg\installed\x64-windows-static\bin\openssl.cfg
- C:\vcpkg\installed\x64-windows-static

This must show where OpenSSL is installed. The one installed here must be listed first if multiple versions are installed.

```PowerShell
Get-Command openssl -All

CommandType     Name                                               Version    Source  
-----------     ----                                               -------    ------  
Application     openssl.exe                                        3.4.0.0    C:\vcpkg\installed\x64-windows-static\bin\openssl.exe  
Application     openssl.exe                                        1.1.1.9    C:\Strawberry\c\bin\openssl.exe  
```

# Install Rust

Next, we need to install support for the Rust language 

```PowerShell
winget install --id Rustlang.Rustup
```
sample ouput:
```
PS C:\Users\leet\src\vcpkg> winget install --id Rustlang.Rustup
Found Rustup: the Rust toolchain installer [Rustlang.Rustup] Version 1.27.1
This application is licensed to you by its owner.
Microsoft is not responsible for, nor does it grant any licenses to, third-party packages.
Downloading https://static.rust-lang.org/rustup/archive/1.27.1/x86_64-pc-windows-msvc/rustup-init.exe
  ██████████████████████████████  8.53 MB / 8.53 MB
Successfully verified installer hash
Starting package install...
Successfully installed
```

# Get the Tari code base
Finally, we can pull down the Tari code base and build Tari. First, clone the repo from the [official project](https://github.com/tari-project/tari/) in your folder of choice. In the example below, we're using a ```src``` folder as the location to store our repos:

```PowerShell
cd src
git clone https://github.com/tari-project/tari.git
cd tari
```

## Basic Test Build Tari Tools
Finally, you should be able to build the Tari tools. In previous steps, we've set the environmental variables for vcpkg and OpenSSL so while the below steps aren't necessary, setting them locally prior to the run will ensure you are pointing to the correct paths.

Again, either via the IDE terminal or Powershell, run the following commands (making sure you are currently in the ```tari``` repo folder created in the previous step):

```PowerShell
setx /m VCPKG_ROOT "C:\vcpkg"
setx /m OPENSSL_DIR "C:\vcpkg\packages\openssl_x64-windows-static"
```

Once you've set your environment variables, we can build the tools:

```Powershell
"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
cargo build --release

# Build Tari Tools
```PowerShell
$Env:VCPKG_ROOT = 'C:\vcpkg'
$Env:OPENSSL_DIR = 'C:\vcpkg\packages\openssl_x64-windows-static'
cargo build --release --bin minotari_miner
```
sample output:
```
PS C:\Users\leet> cd src\tari
PS C:\Users\leet\src\tari> cargo build --release --bin minotari_miner
info: syncing channel updates for 'stable-x86_64-pc-windows-msvc'
info: latest update on 2024-07-07, rust version 1.81.0-nightly (ed7e35f34 2024-07-06)
info: downloading component 'cargo'
info: downloading component 'clippy'
info: downloading component 'rust-docs'
info: downloading component 'rust-std'
info: downloading component 'rustc'
info: downloading component 'rustfmt'
info: installing component 'cargo'
info: installing component 'clippy'
info: installing component 'rust-docs'
info: installing component 'rust-std'
info: installing component 'rustc'
info: installing component 'rustfmt'
    Updating git repository `https://github.com/tari-project/lmdb-rs`
    Updating git submodule `https://github.com/LMDB/lmdb.git`
    Updating crates.io index
    Updating git repository `https://github.com/Zondax/ledger-rs`
 Downloading 516 crates
```

This will build the Minotari miner executable in your ```releases``` folder for the repo. Note that the ```minotari_miner``` is just one of several tools that are available. Others include the ```minotari_node``` and ```minotari_console_wallet```. You can review the project for more details on each of these.
