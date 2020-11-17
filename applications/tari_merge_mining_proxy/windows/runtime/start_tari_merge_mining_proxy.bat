@echo off

echo.
echo Set up environment variables
echo ----------------------------
rem These is the merge mining proxy executable name
set my_exe=tari_merge_mining_proxy.exe

rem This is the location of the configuration and identity files
set config_path=%~dp0..\config
echo config_path = %config_path%

rem The default location for the merge mining proxy executable
set my_exe_path=%~dp0
if %my_exe_path:~-1%==\ set my_exe_path=%my_exe_path:~0,-1%
echo my_exe_path = %my_exe_path%

rem The base folder where the log files will be located
set base_path=%~dp0..
echo base_path   = %base_path%

echo.
echo Run the merge mining proxy
echo --------------------------
call "%my_exe_path%\run_the_merge_mining_proxy.bat"

goto END:


:END
echo.
pause
