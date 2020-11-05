@echo off

CHOICE /C YN /M "Enable Mining?"
IF ERRORLEVEL 2 goto :no_mining

CHOICE /C YN /M "Merged Mining?"
IF ERRORLEVEL 1 goto :merged_mining
IF ERRORLEVEL 2 goto :mining

:no_mining
call start_tor.bat
call start_tari_base_node.bat
goto :end

:mining
call start_tor.bat
call start_tari_base_node.bat --enable_mining
goto :end

:merged_mining
call start_tor.bat
call start_tari_base_node.bat
call start_tari_console_wallet.bat
call start_tari_merge_mining_proxy.bat
call start_xmrig.bat
goto :end

:end
pause
