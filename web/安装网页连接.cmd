@echo off
set "YAS_WEB_SETUP_EXE=%~dp0yas_artifact.exe"
powershell.exe -NoProfile -Command "try { $p = Start-Process -FilePath $env:YAS_WEB_SETUP_EXE -ArgumentList '--install-web' -Verb RunAs -PassThru -Wait; exit $p.ExitCode } catch { Write-Host $_; exit 1 }"
if errorlevel 1 (
    echo Installation failed. Close the running YAS web service and try again.
) else (
    echo Installed. Return to the artifact website and connect YAS.
)
pause
