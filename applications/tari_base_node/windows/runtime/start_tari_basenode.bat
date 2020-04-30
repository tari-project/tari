@echo.
@echo Set up environment variables
@echo ----------------------------
@rem This is the basenode executable name
@set my_exe=tari_base_node.exe

@rem This is the location of the configuration and identity files
@set config_path=%~dp0..\config
@echo config_path=%config_path%

@rem This is the basenode executable location, or leave blank if in path
@set my_exe_path=%~dp0
@rem set my_exe_path=
@if %my_exe_path:~-1%==\ set my_exe_path=%my_exe_path:~0,-1%
@echo my_exe_path=%my_exe_path%

@rem The base folder where the database and log files will be located
@set base_path=%~dp0..
@echo base_path=%base_path%

@echo.
@echo Start Tor Services
@echo ----------------------------
@call %my_exe_path%\start_tor.bat
@if [%errorlevel%]==[10101] goto :END

@echo.
@echo Run the base node
@echo -----------------
@call %my_exe_path%\run_the_base_node.bat

@goto END:


:END
@echo.
@pause

