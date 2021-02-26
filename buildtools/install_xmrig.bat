@echo off

rem Control variables
rem - XMRig latest {Note: `powershell` cannot `expand-archive` to `C:\Program Files (x86)`}
rem   - Download `XMRig` at `https://github.com/xmrig/xmrig/releases/`

set xmrig_zip=xmrig-win64.zip
set xmrig_folder=%USERPROFILE%\.xmrig
set xmrig_runtime=xmrig.exe
set xmrig_repo=https://api.github.com/repos/xmrig/xmrig/releases/latest

echo Downloading and installing XMRig...
echo.

rem Install dependencies
call :INSTALL_XMRIG
goto END:

:INSTALL_XMRIG
rem Download and install
del /f "%TEMP%\%xmrig_zip%" 2>null
powershell "Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process; .\get_xmrig_win.ps1"
powershell Expand-Archive -Force -LiteralPath "%TEMP%\%xmrig_zip%" -DestinationPath '%xmrig_folder%'
powershell "Get-Childitem -File -Recurse '%xmrig_folder%\' | Move-Item  -Force -Destination '%xmrig_folder%'"
powershell "Get-Childitem -Directory $env:USERPROFILE\.xmrig | Remove-item -Force"
rem Set XMRig environment variables
set TARI_XMRIG_DIR=%xmrig_folder%
setx TARI_XMRIG_DIR %TARI_XMRIG_DIR%
setx /m USERNAME %USERNAME%

rem Test installation
if not exist "%TARI_XMRIG_DIR%\%xmrig_runtime%" (
    echo.
    echo Problem with XMrig installation, "%xmrig_runtime%" not found!
    echo {Please try installing this dependency using the manual procedure described in the README file.}
    echo.
    pause
) else (
    echo.
    echo XMRig installation found at "%TARI_XMRIG_DIR%"
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
