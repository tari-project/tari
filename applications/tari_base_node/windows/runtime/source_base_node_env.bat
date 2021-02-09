@echo off
title Tari Base Node

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
if ["%sqlite_runtime%"]==[""] (
    echo Problem with "sqlite_runtime" environment variable: '%sqlite_runtime%'
    pause
    exit /b 10101
)
if ["%openssl_runtime_1%"]==[""] (
    echo Problem with "sopenssl_runtime" environment variable: '%openssl_runtime_1%'
    pause
    exit /b 10101
)
if ["%openssl_runtime_2%"]==[""] (
    echo Problem with "sopenssl_runtime" environment variable: '%openssl_runtime_2%'
    pause
    exit /b 10101
)

rem Verify SQLite's location and prepend the default location to the system path if it exist
if exist "%TARI_SQLITE_DIR%\%sqlite_runtime%" (
    set "path=%TARI_SQLITE_DIR%;%path%"
    echo.
    echo Default location of "%sqlite_runtime%" prepended to the system path
) else if exist "%USERPROFILE%\.sqlite\%sqlite_runtime%" (
    set "path=%USERPROFILE%\.sqlite;%path%"
    echo.
    echo Default location of "%sqlite_runtime%" prepended to the system path
) else (
    set FOUND=
    for %%X in (%sqlite_runtime%) do (set FOUND=%%~$PATH:X)
    if defined FOUND (
        echo.
        echo "%sqlite_runtime%" found in system path:
        where "%sqlite_runtime%"
    ) else (
        echo.
        echo Note: "%sqlite_runtime%" not found in the default location or in the system path; this may be a problem
		if ["%TARI_SQLITE_DIR%"]==["%USERPROFILE%\.sqlite"] (
			echo       {default location: tried "%TARI_SQLITE_DIR%"}
		) else (
			echo       {default location: tried "%TARI_SQLITE_DIR%" and "%USERPROFILE%\.sqlite"}
		)
        echo.
        pause
    )
)

rem Verify OpenSSL's location
set FOUND_OPENSSL=
if exist "%my_exe_path%\%openssl_runtime_1%" (
    if exist "%my_exe_path%\%openssl_runtime_2%" (
        echo.
        echo Using OpenSSL dlls found in "%my_exe_path%"
        echo.
        set FOUND_OPENSSL=true
    )
)
if not defined FOUND_OPENSSL (
    set FOUND_1=
    set FOUND_2=
    for %%X in (%openssl_runtime_1%) do (set FOUND_1=%%~$PATH:X)
    for %%X in (%openssl_runtime_2%) do (set FOUND_2=%%~$PATH:X)
    if defined FOUND_1 (
        if defined FOUND_2 (
            set FOUND_OPENSSL=true
            echo.
            echo OpenSSL dlls found in system path:
            where "%openssl_runtime_1%"
            where "%openssl_runtime_2%"
        )
    )
)
if not defined FOUND_OPENSSL (
    echo.
    echo Note: OpenSSL dlls not found in "%my_exe_path%" or in the system path; this may be a problem
    echo.
    pause
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
    "%base_node%" --create-id --init --config "%config_path%\config.toml" --log_config "%config_path%\log4rs_base_node.yml" --base-path "%base_path%"
    echo.
    echo.
    echo Created "%config_path%\base_node_id.json".
    echo Created "%config_path%\wallet_id.json".
    echo.
) else (
    echo.
    echo.
    echo Using existing "%config_path%\base_node_id.json"
    echo.
)
if not exist "%config_path%\wallet_id.json" (
    cd "%base_path%"
    "%base_node%" --create-id --init --config "%config_path%\config.toml" --log_config "%config_path%\log4rs_base_node.yml" --base-path "%base_path%"
    echo.
    echo.
    echo Created "%config_path%\base_node_id.json".
    echo Created "%config_path%\wallet_id.json".
    echo.
) else (
    echo.
    echo.
    echo Using existing "%config_path%\wallet_id.json"
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
"%base_node%" --config "%config_path%\config.toml" --log_config "%config_path%\log4rs_base_node.yml" --base-path "%base_path%" %mining_flag%
