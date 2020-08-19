@echo off

rem Control variables
rem - Tor Services {Note: `powershell` cannot `expand-archive` to `C:\Program Files (x86)`}
rem   - Download 'Windows Expert Bundle' at https://archive.torproject.org/tor-package-archive/torbrowser/, 
rem     e.g. `9.5.3/tor-win64-0.4.3.6.zip`

set tor_version=9.5.3
set tor_zip=tor-win64-0.4.3.6.zip
set tor_folder=%USERPROFILE%\.tor_services
set tor_runtime=tor.exe

echo Downloading and installing Tor Services...
echo.

rem Install dependencies
call :INSTALL_TOR_SERVICES
goto END:

:INSTALL_TOR_SERVICES
rem Download install file
powershell wget  https://www.torproject.org/dist/torbrowser/%tor_version%/%tor_zip% -outfile "%TEMP%\%tor_zip%"
rem Install 
powershell expand-archive -Force -LiteralPath "%TEMP%\%tor_zip%" -DestinationPath "%tor_folder%"
rem Set Tari environment variables
set TARI_TOR_SERVICES_DIR=%tor_folder%\Tor
setx TARI_TOR_SERVICES_DIR %TARI_TOR_SERVICES_DIR%
setx /m USERNAME %USERNAME%

rem Test installation
if not exist "%TARI_TOR_SERVICES_DIR%\%tor_runtime%" (
    echo.
    echo Problem with Tor Services installation, "%tor_runtime%" not found!
    echo {Please try installing this dependency using the manual procedure described in the README file.}
    echo.
    pause
) else (
    echo.
    echo Tor Services installation found at "%TARI_TOR_SERVICES_DIR%"
)
goto :eof

:END
echo.
if not [%1]==[NO_PAUSE] (
    pause
) else (
    ping -n 5 localhost>nul
)
if [%errorlevel%]==[10101] exit
