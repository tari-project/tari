echo off

rem Control variables
rem - OpenSSL v.1.1.1 latest {Note: `powershell` cannot `expand-archive` to `C:\Program Files (x86)`}
rem   - Download `OpenSSL` at `https://slproweb.com/products/Win32OpenSSL.html`

set openssl_install_file=openssl-win64.exe
set openssl_repo=https://slproweb.com
set openssl_downloads=%openssl_repo%/products/Win32OpenSSL.html

echo Downloading and installing OpenSSL...
echo.

rem Install dependencies
call :INSTALL_OPEN_SSL
goto END:

:INSTALL_OPEN_SSL
rem Download install file
del /f "%TEMP%\%openssl_install_file%" 2>null
powershell "Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process; .\get-openssl-win.ps1"
rem Install
"%TEMP%\%openssl_install_file%"
goto :eof

:END
echo.
if not [%1]==[NO_PAUSE] (
    pause
) else (
    ping -n 5 localhost>nul
)
if [%errorlevel%]==[10101] exit
