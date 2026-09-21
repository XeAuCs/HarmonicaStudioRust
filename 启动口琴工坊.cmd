@echo off
setlocal EnableExtensions DisableDelayedExpansion
chcp 65001 >nul
if not exist "%~dp0app\HarmonicaStudio\HarmonicaStudio.exe" goto missing
start "" /D "%~dp0app\HarmonicaStudio" "%~dp0app\HarmonicaStudio\HarmonicaStudio.exe"
if errorlevel 1 goto launch_failed
exit /b 0

:missing
echo 找不到口琴工坊程序，请先运行“打包.cmd”生成便携版。
pause
exit /b 1

:launch_failed
echo 无法启动口琴工坊，请检查程序文件是否完整。
pause
exit /b 1
