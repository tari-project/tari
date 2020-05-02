@rem Control variables
@rem - Redistributable for Visual Studio 2019
@set vc_redist_install=vc_redist.x64.exe

@echo Downloading and installing Redistributable for Visual Studio 2019...
@echo.

@rem Determine if running as administrator
@call :TEST_ADMINISTRATOR
@if [%errorlevel%]==[10101] goto :END

@rem Install dependencies
@call :INSTALL_VS2019_REDIST
@goto END:

:INSTALL_VS2019_REDIST
@rem Download install file
@powershell wget https://aka.ms/vs/16/release/%vc_redist_install% -outfile "%TEMP%\%vc_redist_install%"
@rem Install
@"%TEMP%\%vc_redist_install%"
@goto :eof

:TEST_ADMINISTRATOR
@echo.
@set guid=%random%%random%-%random%-%random%-%random%-%random%%random%%random%
@mkdir %WINDIR%\%guid%>nul 2>&1
@rmdir %WINDIR%\%guid%>nul 2>&1
@if %ERRORLEVEL% equ 0 (
    @echo Administrator OK
    @echo.
) else (
    @echo Please run as administrator {hint: Right click, then "Run as administrator"}
    @echo.
    @exit /b 10101
)
@goto :eof

:END
@echo.
@if not [%1]==[NO_PAUSE] (
    @pause
) else (
    @ping -n 3 localhost>nul
)
@@if [%errorlevel%]==[10101] exit