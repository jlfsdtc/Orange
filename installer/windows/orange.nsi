; Orange NSIS installer
;
; Build:
;   makensis -DVERSION=0.1.0 -DARCH=x64 installer/windows/orange.nsi
; Inputs (expected paths, relative to repo root):
;   target\release\orange.exe
;   target\release\orange-grep.exe
;   assets\icons\orange.ico

!ifndef VERSION
    !define VERSION "0.0.0"
!endif
!ifndef ARCH
    !define ARCH "x64"
!endif

Name "Orange ${VERSION}"
OutFile "..\..\target\windows\orange-${VERSION}-${ARCH}-setup.exe"
Unicode true
SetCompressor /SOLID lzma

!include "MUI2.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON "..\..\assets\icons\orange.ico"
!define MUI_UNICON "..\..\assets\icons\orange.ico"

InstallDir "$PROGRAMFILES64\Orange"
InstallDirRegKey HKLM "Software\Orange" "InstallDir"
RequestExecutionLevel admin

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Orange"
VIAddVersionKey "FileDescription" "Orange - fast log viewer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "GPL-3.0-or-later"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Orange" SecCore
    SectionIn RO
    SetOutPath "$INSTDIR"

    File "..\..\target\release\orange.exe"
    File "..\..\target\release\orange-grep.exe"
    File "..\..\assets\icons\orange.ico"

    ; Registry uninstall entry
    WriteRegStr HKLM "Software\Orange" "InstallDir" "$INSTDIR"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "DisplayName" "Orange"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "DisplayVersion" "${VERSION}"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "DisplayIcon" "$INSTDIR\orange.ico"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "Publisher" "Orange contributors"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
    WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "NoModify" 1
    WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange" "NoRepair" 1

    ; Add to PATH (per-machine)
    EnVar::SetHKLM
    EnVar::AddValue "Path" "$INSTDIR"
    Pop $0

    WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Start Menu shortcut" SecStartMenu
    CreateDirectory "$SMPROGRAMS\Orange"
    CreateShortcut "$SMPROGRAMS\Orange\Orange.lnk" "$INSTDIR\orange.exe" "" "$INSTDIR\orange.ico"
    CreateShortcut "$SMPROGRAMS\Orange\Uninstall Orange.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

Section "Desktop shortcut" SecDesktop
    CreateShortcut "$DESKTOP\Orange.lnk" "$INSTDIR\orange.exe" "" "$INSTDIR\orange.ico"
SectionEnd

Section "Uninstall"
    EnVar::SetHKLM
    EnVar::DeleteValue "Path" "$INSTDIR"
    Pop $0

    Delete "$INSTDIR\orange.exe"
    Delete "$INSTDIR\orange-grep.exe"
    Delete "$INSTDIR\orange.ico"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    Delete "$SMPROGRAMS\Orange\Orange.lnk"
    Delete "$SMPROGRAMS\Orange\Uninstall Orange.lnk"
    RMDir "$SMPROGRAMS\Orange"
    Delete "$DESKTOP\Orange.lnk"

    DeleteRegKey HKLM "Software\Orange"
    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Orange"
SectionEnd
