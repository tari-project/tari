@echo off

echo.
echo Set up environment variables
echo ----------------------------
rem These are the miner executable and SQLite dynamic link library names
set my_exe=minotari_miner.exe

rem This is the location of the configuration and identity files
set config_path=%~dp0..\config
echo config_path = %config_path%

rem The default location for the miner executable
set my_exe_path=%~dp0
if %my_exe_path:~-1%==\ set my_exe_path=%my_exe_path:~0,-1%
echo my_exe_path = %my_exe_path%

rem The base folder where the database and log files will be located
set base_path=%~dp0..
echo base_path   = %base_path%

echo.
echo Run the miner
echo ----------------------
call "%my_exe_path%\source_miner_env.bat"

goto END:


:END
echo.
pause
