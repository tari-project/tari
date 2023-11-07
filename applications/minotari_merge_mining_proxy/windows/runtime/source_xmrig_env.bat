@echo off
rem Verify arguments
if ["%config_path%"]==[""] (
    echo Problem with "config_path" environment variable: '%config_path%'
    pause
    exit /b 10101
)
if not exist "%config_path%" (
    echo Path as per "config_path" environment variable not found: '%config_path%'
    pause
    exit /b 10101
)
if ["%my_exe%"]==[""] (
    echo Problem with "my_exe" environment variable: '%my_exe%'
    pause
    exit /b 10101
)

rem Find the XMrig executable
set xmrig=%TARI_XMRIG_DIR%\%my_exe%
if exist "%TARI_XMRIG_DIR%\%my_exe%" (
    set xmrig=%TARI_XMRIG_DIR%\%my_exe%
    echo.
    echo Using "%my_exe%" found in %TARI_XMRIG_DIR%
    echo.
) else (
    echo.
    echo Runtime "%my_exe%" not found in %TARI_XMRIG_DIR%
    echo.
    pause
    exit /b 10101
)

rem Copy the config file to the XMRig folder
copy /y /v "%config_path%\xmrig_config_example_stagenet.json" "%TARI_XMRIG_DIR%\config.json"

rem Run
"%xmrig%"
