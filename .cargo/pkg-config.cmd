@echo off
setlocal

if not defined VCPKG_ROOT (
    echo VCPKG_ROOT is not set. 1>&2
    echo Set VCPKG_ROOT to your vcpkg installation directory before building. 1>&2
    exit /b 1
)

set "VCPKG_TRIPLET=%VCPKG_DEFAULT_TRIPLET%"
if not defined VCPKG_TRIPLET set "VCPKG_TRIPLET=x64-windows"

set "PKGCONF=%VCPKG_ROOT%\installed\%VCPKG_TRIPLET%\tools\pkgconf\pkgconf.exe"
if not exist "%PKGCONF%" (
    echo pkgconf.exe was not found at "%PKGCONF%". 1>&2
    echo Check VCPKG_ROOT and VCPKG_DEFAULT_TRIPLET. 1>&2
    exit /b 1
)

if not defined PKG_CONFIG_PATH (
    set "PKG_CONFIG_PATH=%VCPKG_ROOT%\installed\%VCPKG_TRIPLET%\lib\pkgconfig"
)

"%PKGCONF%" %*
