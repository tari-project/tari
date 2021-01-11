@echo off

rem This is the location of the configuration and identity files
set config_path=%~dp0..\config
echo config_path = %config_path%

rem The default runtime location
set my_exe_path=%~dp0
if %my_exe_path:~-1%==\ set my_exe_path=%my_exe_path:~0,-1%
echo my_exe_path = %my_exe_path%

echo.
choice /C YN /M "Enable Mining?"
if ["%ERRORLEVEL%"]==["2"] (
    goto :NO_MINING
)

echo.
choice /C YN /M "Merged Mining?"
echo.
if ["%ERRORLEVEL%"]==["1"] (
    goto :MERGED_MINING
)
if ["%ERRORLEVEL%"]==["2"] (
    goto :MINING
)

:NO_MINING
echo No mining
set mining_flag=
start cmd /k "%my_exe_path%\start_tari_base_node.bat"
ping -n 7 localhost>nul
start cmd /k "%my_exe_path%\start_tari_console_wallet.bat"
goto :end

:MINING
echo Mining
set mining_flag=--enable_mining
start cmd /k "%my_exe_path%\start_tari_base_node.bat"
ping -n 7 localhost>nul
start cmd /k "%my_exe_path%\start_tari_console_wallet.bat"
goto :end

:MERGED_MINING
echo Merged mining
rem TODO: Problem enclosing these in quotes, to be sorted out
start cmd /k "%my_exe_path%\start_tari_base_node.bat"
ping -n 7 localhost>nul
start cmd /k "%my_exe_path%\start_tari_console_wallet.bat"
ping -n 7 localhost>nul
start cmd /k "%my_exe_path%\start_tari_merge_mining_proxy.bat"
ping -n 7 localhost>nul
start cmd /k "%my_exe_path%\start_xmrig.bat"
goto :end

:end
