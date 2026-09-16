@echo off
rem Verify arguments
if ["%xmrig_config_path%"]==[""] (
    echo Problem with "xmrig_config_path" environment variable: '%xmrig_config_path%'
    pause
    exit /b 10101
)
if not exist "%xmrig_config_path%" (
    echo Path as per "xmrig_config_path" environment variable not found: '%xmrig_config_path%'
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
if not exist "%TARI_XMRIG_DIR%\config.json" (
    copy /y /v "%xmrig_config_path%\xmrig_config_example_mainnet.json" "%TARI_XMRIG_DIR%\config.json"
)

rem Run
"%xmrig%"
