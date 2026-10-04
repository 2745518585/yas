@echo off
set "YAS_WEB_SETUP_EXE=%~dp0yas_artifact.exe"
powershell.exe -NoProfile -Command "try { $p = Start-Process -FilePath $env:YAS_WEB_SETUP_EXE -ArgumentList '--uninstall-web' -Verb RunAs -PassThru -Wait; exit $p.ExitCode } catch { Write-Host $_; exit 1 }"
if errorlevel 1 (
    echo Uninstall failed.
) else (
    echo The YAS web protocol was removed. Close the running YAS web service.
)
pause
