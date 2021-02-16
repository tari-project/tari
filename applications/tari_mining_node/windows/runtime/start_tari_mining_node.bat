@echo off

echo.
echo Set up environment variables
echo ----------------------------
rem These are the mining node executable and SQLite dynamic link library names
set my_exe=tari_mining_node.exe

rem This is the location of the configuration and identity files
set config_path=%~dp0..\config
echo config_path = %config_path%

rem The default location for the mining node executable
set my_exe_path=%~dp0
if %my_exe_path:~-1%==\ set my_exe_path=%my_exe_path:~0,-1%
echo my_exe_path = %my_exe_path%

rem The base folder where the database and log files will be located
set base_path=%~dp0..
echo base_path   = %base_path%

echo.
echo Run the mining node
echo ----------------------
call "%my_exe_path%\source_mining_node_env.bat"

goto END:


:END
echo.
pause
