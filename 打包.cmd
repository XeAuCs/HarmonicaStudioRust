@echo off
setlocal EnableExtensions DisableDelayedExpansion
chcp 65001 >nul
pushd "%~dp0"
if errorlevel 1 goto directory_failed
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build.ps1" -Console -PromptVersion
set "HARMONICA_BUILD_EXIT=%errorlevel%"
popd
echo(按任意键关闭窗口...
pause >nul
exit /b %HARMONICA_BUILD_EXIT%

:directory_failed
echo 无法进入项目目录，请检查文件夹是否可访问。
echo(按任意键关闭窗口...
pause >nul
exit /b 1
