@echo off

echo.
echo Set up environment variables
echo ----------------------------
rem These are the console wallet executable and SQLite dynamic link library names
set my_exe=tari_console_wallet.exe
set sqlite_runtime=sqlite3.dll

rem This is the location of the configuration and identity files
set config_path=%~dp0..\config
echo config_path = %config_path%

rem The default location for the console wallet executable
set my_exe_path=%~dp0
if %my_exe_path:~-1%==\ set my_exe_path=%my_exe_path:~0,-1%
echo my_exe_path = %my_exe_path%

rem The base folder where the database and log files will be located
set base_path=%~dp0..
echo base_path   = %base_path%

echo.
echo Start Tor Services
echo ----------------------------
call "%my_exe_path%\start_tor.bat"
if [%errorlevel%]==[10101] (
    echo.
    echo It seems Tor could not be started properly.
    echo If '%my_exe%' still reports an error:
    echo   - Try to start Tor manually:
    echo     - execute 'start_tor.bat' from '%my_exe_path%', or
    echo     - select 'Tor Services' from the 'Tari - Testnet' menu
    echo   - Wait for '[notice] Bootstrapped 100% (done): Done' in the Tor console
    echo.
    pause
)

echo.
echo Run the console wallet
echo ----------------------
call "%my_exe_path%\run_the_console_wallet.bat"

goto END:


:END
echo.
pause
