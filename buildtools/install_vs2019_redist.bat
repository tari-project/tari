echo off

rem Control variables
rem - Redistributable for Visual Studio 2019
set vc_redist_install=vc_redist.x64.exe

echo Downloading and installing Redistributable for Visual Studio 2019...
echo.

rem Install dependencies
call :INSTALL_VS2019_REDIST
goto END:

:INSTALL_VS2019_REDIST
rem Download install file
del /f "%TEMP%\%vc_redist_install%" 2>null
powershell Invoke-WebRequest https://aka.ms/vs/16/release/%vc_redist_install% -outfile "%TEMP%\%vc_redist_install%"
rem Install
"%TEMP%\%vc_redist_install%"
goto :eof

:END
echo.
if not [%1]==[NO_PAUSE] (
    pause
) else (
    ping -n 5 localhost>nul
)
if [%errorlevel%]==[10101] exit
