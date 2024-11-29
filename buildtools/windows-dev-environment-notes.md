
# Basic build environment setup guide for Windows using winget for quick developer package testing - neither definitive nor exhaustive

Lots of info collected from - https://github.com/KhronosGroup/OpenCL-Guide/blob/main/chapters/getting_started_windows.md

and ```needed for developing Tauri apps.``` - https://v1.tauri.app/v1/guides/getting-started/prerequisites

Need to have ```winget``` installed and working, which requires ```App Installer```, not only installed, but updated to the latest version.

Using Microsoft Edge, open the following URL:

https://www.microsoft.com/p/app-installer/9nblggh4nns1#activetab=pivot:overviewtab

then click the ```App Installer``` install button.

Found that after installing and rebooting Windows and checking for ```App Installer``` updates and appling any outstanding updates. 

Check that ```winget``` is working, run in PowerShell.
```PowerShell
winget list
```
sample of output without ```App Installer``` installed
```
PS C:\Users\leet> winget list
  |
PS C:\Users\leet>
```

sample output where ```winget``` has not be run yet:
```PowerShell
PS C:\Users\leet> winget list
Failed in attempting to update the source: winget
The `msstore` source requires that you view the following agreements before using.
Terms of Transaction: https://aka.ms/microsoft-store-terms-of-transaction
The source requires the current machine's 2-letter geographic region to be sent to the backend service to function properly (ex. "US").

Do you agree to all the source agreements terms?
[Y] Yes  [N] No: y
Failed when searching source; results will not be included: winget
Name                                           Id                                                   Version
-----------------------------------------------------------------------------------------------------------------------
Clipchamp                                      Clipchamp.Clipchamp_yxz26nhyzhsrt                    2.2.8.0
Microsoft Edge                                 Microsoft Edge                                       130.0.2849.80
Microsoft Edge WebView2 Runtime                Microsoft EdgeWebView                                130.0.2849.80
```
please notice ```Failed when searching source; results will not be included: winget```, normally means that ```App Installer``` needs to be updated.

sample output where ```App Installer``` is installed, but not updated to the latest:
```
PS C:\Users\leet> winget list
Failed in attempting to update the source: winget
Failed when searching source; results will not be included: winget
```

sample of output where ```winget``` is ready to be used for installing tools:
```
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

Then we can start installing components that will be needed in Compiling ```The Tari protocol tools``` locally

# Install Visual Studio BuildTools 2022
```PowerShell
winget install "Visual Studio BuildTools 2022"
```
sample output would look something like:
```
PS C:\Users\leet> winget install "Visual Studio BuildTools 2022"
Found Visual Studio BuildTools 2022 [Microsoft.VisualStudio.2022.BuildTools] Version 17.11.5
This application is licensed to you by its owner.
Microsoft is not responsible for, nor does it grant any licenses to, third-party packages.
Downloading https://download.visualstudio.microsoft.com/download/pr/69e24482-3b48-44d3-af65-51f866a08313/471c9a89fa8ba27d356748ae0cf25eb1f362184992dc0bb6e9ccf10178c43c27/vs_BuildTools.exe
  ██████████████████████████████  4.22 MB / 4.22 MB
Successfully verified installer hash
Starting package install...
Successfully installed
```

# Install Visual Studio components for Windows 11
```PowerShell
& "C:\Program Files (x86)\Microsoft Visual Studio\Installer\setup.exe" install --passive --norestart --productId Microsoft.VisualStudio.Product.BuildTools --channelId VisualStudio.17.Release --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.VC.Redist.14.Latest --add Microsoft.VisualStudio.Component.Windows11SDK.26100 --add Microsoft.VisualStudio.Component.VC.CMake.Project --add Microsoft.VisualStudio.Component.VC.CoreBuildTools --add Microsoft.VisualStudio.Component.VC.CoreIde --add Microsoft.VisualStudio.Component.VC.Redist.14.Latest --add Microsoft.VisualStudio.ComponentGroup.NativeDesktop.Core
````
sample of the begining of output:
```
PS C:\Users\leet> & "C:\Program Files (x86)\Microsoft Visual Studio\Installer\setup.exe" install --passive --norestart --productId Microsoft.VisualStudio.Product.BuildTools --channelId VisualStudio.17.Release --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.VC.Redist.14.Latest --add Microsoft.VisualStudio.Component.Windows11SDK.22000
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

# Install git - https://git-scm.com/downloads/win
```PowerShell
winget install --id Git.Git -e --source winget
```
sample output:
```
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

# Install Windows chocolatey package manager, helps with easy installation of additional components (protobuf)
```PowerShell
winget install --id chocolatey.chocolatey
```
sample output:
```
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

# Install Protobuf with chocolatey
Use a new PowerShell console, as choco will not be in the current console path and seem broken.
```PowerShell
choco upgrade protoc -y
```
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

chocolatey-compatibility.extension v1.0.0 [Approved]
chocolatey-compatibility.extension package files upgrade completed. Performing other installation steps.
 Installed/updated chocolatey-compatibility extensions.
 The upgrade of chocolatey-compatibility.extension was successful.
  Deployed to 'C:\ProgramData\chocolatey\extensions\chocolatey-compatibility'
Downloading package from source 'https://community.chocolatey.org/api/v2/'
Progress: Downloading chocolatey-core.extension 1.4.0... 100%

chocolatey-core.extension v1.4.0 [Approved]
chocolatey-core.extension package files upgrade completed. Performing other installation steps.
 Installed/updated chocolatey-core extensions.
 The upgrade of chocolatey-core.extension was successful.
  Deployed to 'C:\ProgramData\chocolatey\extensions\chocolatey-core'
Downloading package from source 'https://community.chocolatey.org/api/v2/'
Progress: Downloading protoc 28.3.0... 100%

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

# Install Windows vcpkg package manager, helps with easy installation of additional components (openssl)
```PowerShell
git clone https://github.com/microsoft/vcpkg.git \vcpkg
cd \vcpkg
.\bootstrap-vcpkg.bat
```
sample output:
```
PS C:\Users\leet> git clone https://github.com/microsoft/vcpkg.git C:\
>>
fatal: destination path 'C:' already exists and is not an empty directory.
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

See LICENSE.txt for license information.
Telemetry
---------
vcpkg collects usage data in order to help us improve your experience.
The data collected by Microsoft is anonymous.
You can opt-out of telemetry by re-running the bootstrap-vcpkg script with -disableMetrics,
passing --disable-metrics to vcpkg on the command line,
or by setting the VCPKG_DISABLE_METRICS environment variable.

Read more about vcpkg telemetry at docs/about/privacy.md
```

# Install Openssl with vcpkg
```PowerShell
$Env:Path += ';C:\vcpkg'
vcpkg install openssl:x64-windows-static
```
sample output:
```
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
A suitable version of 7zip was not found (required v24.8.0).
Downloading 7z2408-extra.7z
Successfully downloaded 7z2408-extra.7z.
Extracting 7zip...
A suitable version of 7zr was not found (required v24.8.0).
Downloading 424196f2-7zr.exe
Successfully downloaded 424196f2-7zr.exe.
The following packages will be built and installed:
    openssl:x64-windows-static@3.4.0
  * vcpkg-cmake:x64-windows@2024-04-23
  * vcpkg-cmake-config:x64-windows@2024-05-23
  * vcpkg-cmake-get-vars:x64-windows@2024-09-22
Additional packages (*) will be modified to complete this operation.
Detecting compiler hash for triplet x64-windows...
A suitable version of powershell-core was not found (required v7.2.24).
Downloading PowerShell-7.2.24-win-x64.zip
Successfully downloaded PowerShell-7.2.24-win-x64.zip.
Extracting powershell-core...
Compiler found: C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.42.34433/bin/Hostx64/x64/cl.exe
Detecting compiler hash for triplet x64-windows-static...
Compiler found: C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.42.34433/bin/Hostx64/x64/cl.exe
Restored 0 package(s) from C:\Users\leet\AppData\Local\vcpkg\archives in 523 us. Use --debug to see more details.
Installing 1/4 vcpkg-cmake:x64-windows@2024-04-23...
Building vcpkg-cmake:x64-windows@2024-04-23...
-- Installing: C:/vcpkg/packages/vcpkg-cmake_x64-windows/share/vcpkg-cmake/vcpkg_cmake_configure.cmake
-- Installing: C:/vcpkg/packages/vcpkg-cmake_x64-windows/share/vcpkg-cmake/vcpkg_cmake_build.cmake
-- Installing: C:/vcpkg/packages/vcpkg-cmake_x64-windows/share/vcpkg-cmake/vcpkg_cmake_install.cmake
-- Installing: C:/vcpkg/packages/vcpkg-cmake_x64-windows/share/vcpkg-cmake/vcpkg-port-config.cmake
-- Installing: C:/vcpkg/packages/vcpkg-cmake_x64-windows/share/vcpkg-cmake/copyright
-- Performing post-build validation
Stored binaries in 1 destinations in 123 ms.
Elapsed time to handle vcpkg-cmake:x64-windows: 428 ms
vcpkg-cmake:x64-windows package ABI: 1c9cd6d15b6bd6353941d2a7172da60b44407b254c8a998e11ac63a691d88c8c
Installing 2/4 vcpkg-cmake-config:x64-windows@2024-05-23...
Building vcpkg-cmake-config:x64-windows@2024-05-23...
-- Installing: C:/vcpkg/packages/vcpkg-cmake-config_x64-windows/share/vcpkg-cmake-config/vcpkg_cmake_config_fixup.cmake
-- Installing: C:/vcpkg/packages/vcpkg-cmake-config_x64-windows/share/vcpkg-cmake-config/vcpkg-port-config.cmake
-- Installing: C:/vcpkg/packages/vcpkg-cmake-config_x64-windows/share/vcpkg-cmake-config/copyright
-- Skipping post-build validation due to VCPKG_POLICY_EMPTY_PACKAGE
Stored binaries in 1 destinations in 139 ms.
Elapsed time to handle vcpkg-cmake-config:x64-windows: 371 ms
vcpkg-cmake-config:x64-windows package ABI: 3d79309c04958a43ccac3d839dceb8b3bf77fe6483ba5d7139e011f522841777
Installing 3/4 vcpkg-cmake-get-vars:x64-windows@2024-09-22...
Building vcpkg-cmake-get-vars:x64-windows@2024-09-22...
-- Installing: C:/vcpkg/packages/vcpkg-cmake-get-vars_x64-windows/share/vcpkg-cmake-get-vars/vcpkg_cmake_get_vars.cmake
-- Installing: C:/vcpkg/packages/vcpkg-cmake-get-vars_x64-windows/share/vcpkg-cmake-get-vars/cmake_get_vars
-- Installing: C:/vcpkg/packages/vcpkg-cmake-get-vars_x64-windows/share/vcpkg-cmake-get-vars/cmake_get_vars/CMakeLists.txt
-- Installing: C:/vcpkg/packages/vcpkg-cmake-get-vars_x64-windows/share/vcpkg-cmake-get-vars/vcpkg-port-config.cmake
-- Installing: C:/vcpkg/packages/vcpkg-cmake-get-vars_x64-windows/share/vcpkg-cmake-get-vars/copyright
-- Performing post-build validation
Stored binaries in 1 destinations in 144 ms.
Elapsed time to handle vcpkg-cmake-get-vars:x64-windows: 375 ms
vcpkg-cmake-get-vars:x64-windows package ABI: 06e4bf7043f81750b3aaa7aa31a68dec84d1b064d55b6130ffea76f8ce300ffe
Installing 4/4 openssl:x64-windows-static@3.4.0...
Building openssl:x64-windows-static@3.4.0...
Downloading openssl-openssl-openssl-3.4.0.tar.gz
Successfully downloaded openssl-openssl-openssl-3.4.0.tar.gz.
-- Extracting source C:/vcpkg/downloads/openssl-openssl-openssl-3.4.0.tar.gz
-- Applying patch cmake-config.patch
-- Applying patch command-line-length.patch
-- Applying patch script-prefix.patch
-- Applying patch asm-armcap.patch
-- Applying patch windows/install-layout.patch
-- Applying patch windows/install-pdbs.patch
-- Applying patch unix/android-cc.patch
-- Applying patch unix/move-openssldir.patch
-- Applying patch unix/no-empty-dirs.patch
-- Applying patch unix/no-static-libs-for-shared.patch
-- Using source at C:/vcpkg/buildtrees/openssl/src/nssl-3.4.0-821e8e5bdc.clean
Downloading strawberry-perl-5.40.0.1-64bit-portable.zip
Successfully downloaded strawberry-perl-5.40.0.1-64bit-portable.zip.
-- Found external ninja('1.12.1').
-- Getting CMake variables for x64-windows-static
Downloading nasm-2.16.01-win64.zip
Successfully downloaded nasm-2.16.01-win64.zip.
-- Getting CMake variables for x64-windows-static
Downloading jom_1_1_4.zip
Successfully downloaded jom_1_1_4.zip.
-- Prerunning x64-windows-static-dbg
-- Building x64-windows-static-dbg
-- Prerunning x64-windows-static-rel
-- Building x64-windows-static-rel
-- Fixing pkgconfig file: C:/vcpkg/packages/openssl_x64-windows-static/lib/pkgconfig/libcrypto.pc
-- Fixing pkgconfig file: C:/vcpkg/packages/openssl_x64-windows-static/lib/pkgconfig/libssl.pc
-- Fixing pkgconfig file: C:/vcpkg/packages/openssl_x64-windows-static/lib/pkgconfig/openssl.pc
Downloading msys2-mingw-w64-x86_64-pkgconf-1~2.3.0-1-any.pkg.tar.zst
Successfully downloaded msys2-mingw-w64-x86_64-pkgconf-1~2.3.0-1-any.pkg.tar.zst.
Downloading msys2-msys2-runtime-3.5.4-2-x86_64.pkg.tar.zst
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

# Install rust
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
```PowerShell
cd src
git clone https://github.com/tari-project/tari.git
cd tari
```
sample output:
```
PS C:\Users\leet\src> git clone https://github.com/tari-project/tari.git
>>
Cloning into 'tari'...
remote: Enumerating objects: 133401, done.
remote: Counting objects: 100% (7577/7577), done.
remote: Compressing objects: 100% (3635/3635), done.
remote: Total 133401 (delta 4830), reused 6216 (delta 3900), pack-reused 125824 (from 1)
Receiving objects: 100% (133401/133401), 144.04 MiB | 5.98 MiB/s, done.
Resolving deltas: 100% (99974/99974), done.
Updating files: 100% (1786/1786), done.
```

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
info: syncing channel updates for 'nightly-2024-07-07-x86_64-pc-windows-msvc'
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
