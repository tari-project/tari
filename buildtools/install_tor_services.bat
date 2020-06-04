@rem Control variables
@rem - Tor Services {Note: `powershell` cannot `expand-archive` to `C:\Program Files (x86)`}
@rem   - Download 'Windows Expert Bundle' at https://www.torproject.org/download/tor/
https://www.torproject.org/dist/torbrowser/9.5/tor-win32-0.4.3.5.zip
@set tor_version=9.5
@set tor_zip=tor-win32-0.4.3.5.zip
@set tor_folder=%USERPROFILE%\.tor_services
@set tor_runtime=tor.exe

@echo Downloading and installing Tor Services...
@echo.

@rem Determine if running as administrator
@call :TEST_ADMINISTRATOR
@if [%errorlevel%]==[10101] goto :END

@rem Install dependencies
@call :INSTALL_TOR_SERVICES
@goto END:

:INSTALL_TOR_SERVICES
@rem Download install file
@powershell wget  https://www.torproject.org/dist/torbrowser/%tor_version%/%tor_zip% -outfile "%TEMP%\%tor_zip%"
@rem Install 
@powershell expand-archive -Force -LiteralPath "%TEMP%\%tor_zip%" -DestinationPath "%tor_folder%"
@rem Set Tari environment variables
@set TARI_TOR_SERVICES_DIR=%tor_folder%\Tor
@setx TARI_TOR_SERVICES_DIR %TARI_TOR_SERVICES_DIR%
@setx /m USERNAME %USERNAME%

@rem Test installation
@if not exist "%TARI_TOR_SERVICES_DIR%\%tor_runtime%" (
    @echo.
    @echo Problem with Tor Services installation, "%tor_runtime%" not found!
    @echo {Please try installing this dependency using the manual procedure described in the README file.}
    @echo.
    @pause
) else (
    @echo.
    @echo Tor Services installation found at "%TARI_TOR_SERVICES_DIR%"
)
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