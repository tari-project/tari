@echo off
title Tari Mining Node

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
if ["%base_path%"]==[""] (
    echo Problem with "base_path" environment variable: '%base_path%'
    pause
    exit /b 10101
)
if not exist "%base_path%" (
    echo Path as per "base_path" environment variable not found: '%base_path%'
    pause
    exit /b 10101
)
if ["%my_exe%"]==[""] (
    echo Problem with "my_exe" environment variable: '%my_exe%'
    pause
    exit /b 10101
)

rem Find the mining node executable
if exist "%my_exe_path%\%my_exe%" (
    set mining_node=%my_exe_path%\%my_exe%
    echo.
    echo Using "%my_exe%" found in %my_exe_path%
    echo.
) else (
    if exist "%base_path%\%my_exe%" (
        set mining_node=%base_path%\%my_exe%
        echo.
        echo Using "%my_exe%" found in base_path
        echo.
    ) else (
        set FOUND=
        for %%X in (%my_exe%) do (set FOUND=%%~$PATH:X)
        if defined FOUND (
            set mining_node=%my_exe%
            echo.
            echo Using "%my_exe%" found in system path:
            where "%my_exe%"
            echo.
        ) else (
            echo.
            echo Runtime "%my_exe%" not found in %my_exe_path%, base_path or the system path
            echo.
            pause
            exit /b 10101
        )
    )
)

echo.
echo.
if not exist "%config_path%\log4rs_mining_node.yml" (
    echo Creating new "%config_path%\log4rs_mining_node.yml".
    set INIT_FLAG=--init
) else (
    echo Using existing "%config_path%\log4rs_mining_node.yml"
    set INIT_FLAG=
)
echo.

cd "%base_path%"
"%mining_node%" %INIT_FLAG% --config "%config_path%\config.toml" --log_config "%config_path%\log4rs_mining_node.yml" --base-path "%base_path%"
