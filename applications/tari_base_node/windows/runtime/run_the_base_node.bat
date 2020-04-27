@if [%config_path%]==[] (
    @echo Problem with "config_path" environment variable: %config_path%
    @pause
    @exit /b 10101
)
@if not exist "%config_path%" (
    @echo Path as per "config_path" environment variable not found: %config_path%
    @pause
    @exit /b 10101
)
@if [%base_path%]==[] (
    @echo Problem with "base_path" environment variable: %base_path%
    @pause
    @exit /b 10101
)
@if not exist "%base_path%" (
    @echo Path as per "base_path" environment variable not found: %base_path%
    @pause
    @exit /b 10101
)
@if [%my_exe%]==[] (
    @echo Problem with "my_exe" environment variable: %my_exe%
    @pause
    @exit /b 10101
)
@if exist "%my_exe_path%\%my_exe%" (
    @set base_node=%my_exe_path%\%my_exe%
) else (
    @if exist ".\%my_exe%" (
        @set base_node=.\%my_exe%
    ) else (
        @echo File "%my_exe_path%\%my_exe%" not found
        @for %%X in (%my_exe%) do @(set FOUND=%%~$PATH:X)
        @if defined FOUND (
            @echo.
            @echo Using "%my_exe%" found in path:
            @where "%my_exe%"
            @echo.
            @set base_node=%my_exe%
            @pause
        ) else (
            @pause
            @exit /b 10101
        )
    )
)

@if not exist %config_path%\node_id.json (
    "%base_node%" --create-id --config "%config_path%\windows.toml" --log_config "%config_path%\log4rs.yml" --base-path "%base_path%"
    @echo.
    @echo.
    @echo Created "%config_path%\node_id.json". 
    @echo.
) else (
    @echo.
    @echo.
    @echo Using old "%config_path%\node_id.json"
    @echo.
)
"%base_node%" --config "%config_path%\windows.toml" --log_config "%config_path%\log4rs.yml" --base-path "%base_path%"
