@echo off

echo.
echo Set up environment variables
echo ----------------------------
rem This is the XMRig executable name
set my_exe=xmrig.exe

rem This is the location of the XMRig configuration file
set config_path=%~dp0..\config
echo config_path = %config_path%

rem The default location for the XMRig start file
set my_exe_path=%~dp0
if %my_exe_path:~-1%==\ set my_exe_path=%my_exe_path:~0,-1%
echo my_exe_path = %my_exe_path%

echo.
echo Run XMRig
echo ---------
call "%my_exe_path%\run_xmrig.bat"

goto END:


:END
echo.
pause
