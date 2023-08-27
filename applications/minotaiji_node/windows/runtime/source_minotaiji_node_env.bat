@echo off
title Minotaiji Base Node

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

rem Find the base node executable
if exist "%my_exe_path%\%my_exe%" (
    set base_node=%my_exe_path%\%my_exe%
    echo.
    echo Using "%my_exe%" found in %my_exe_path%
    echo.
) else (
    if exist "%base_path%\%my_exe%" (
        set base_node=%base_path%\%my_exe%
        echo.
        echo Using "%my_exe%" found in base_path
        echo.
    ) else (
        set FOUND=
        for %%X in (%my_exe%) do (set FOUND=%%~$PATH:X)
        if defined FOUND (
            set base_node=%my_exe%
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

rem First time run
if not exist "%config_path%\base_node_id.json" (
    "%base_node%" --init --config "%config_path%\config.toml" --log_config "%config_path%\log4rs_base_node.yml" --base-path "%base_path%"
    echo.
    echo.
    echo Created "%config_path%\base_node_id.json".
    echo.
) else (
    echo.
    echo.
    echo Using existing "%config_path%\base_node_id.json"
    echo.
)
if not exist "%config_path%\log4rs_base_node.yml" (
    cd "%base_path%"
    "%base_node%" --init --config "%config_path%\config.toml" --log_config "%config_path%\log4rs_base_node.yml" --base-path "%base_path%"
    echo.
    echo.
    echo Created "%config_path%\log4rs_base_node.yml".
    echo.
) else (
    echo.
    echo.
    echo Using existing "%config_path%\log4rs_base_node.yml"
    echo.
)

rem Consecutive runs
cd "%base_path%"
"%base_node%" --config "%config_path%\config.toml" --log_config "%config_path%\log4rs_base_node.yml" --base-path "%base_path%"
