; AnyDoc NSIS 安装脚本 (最终版)
Unicode True

!include "MUI2.nsh"
!include "StrFunc.nsh"

Name "AnyDoc"
OutFile "AnyDoc-Setup.exe"
InstallDir "$PROGRAMFILES\AnyDoc"
InstallDirRegKey HKCU "Software\AnyDoc" "InstallPath"
RequestExecutionLevel user

!define MUI_ABORTWARNING

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Section "AnyDoc 核心程序" SecAnyDoc
  SectionIn RO
  SetOutPath "$INSTDIR"

  ; 主程序
  File "anydoc.exe"

  ; 写入卸载程序
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; 注册表
  WriteRegStr HKCU "Software\AnyDoc" "InstallPath" $INSTDIR
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyDoc" "DisplayName" "AnyDoc - 文档转Markdown工具"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyDoc" "DisplayVersion" "0.1.0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyDoc" "Publisher" "AnyDoc Team"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyDoc" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""

  ; 开始菜单快捷方式
  CreateDirectory "$SMPROGRAMS\AnyDoc"
  CreateShortcut "$SMPROGRAMS\AnyDoc\AnyDoc.lnk" "$INSTDIR\anydoc.exe" ""
  CreateShortcut "$SMPROGRAMS\AnyDoc\卸载 AnyDoc.lnk" "$INSTDIR\Uninstall.exe" ""

  ; 桌面快捷方式
  CreateShortcut "$DESKTOP\AnyDoc.lnk" "$INSTDIR\anydoc.exe" ""
SectionEnd

Section "添加到系统 PATH（命令行全局可用）" SecPath
  ; 直接修改用户 PATH
  ReadRegStr $0 HKCU "Environment" "Path"
  ${If} $0 == ""
    WriteRegExpandStr HKCU "Environment" "Path" "$INSTDIR"
  ${Else}
    WriteRegExpandStr HKCU "Environment" "Path" "$INSTDIR;$0"
  ${EndIf}

  ; 通知环境变量更新
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
SectionEnd

Section "Uninstall"
  ; 删除文件
  Delete "$INSTDIR\anydoc.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  ; 删除注册表
  DeleteRegKey HKCU "Software\AnyDoc"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnyDoc"

  ; 删快捷方式
  Delete "$SMPROGRAMS\AnyDoc\AnyDoc.lnk"
  Delete "$SMPROGRAMS\AnyDoc\卸载 AnyDoc.lnk"
  RMDir "$SMPROGRAMS\AnyDoc"
  Delete "$DESKTOP\AnyDoc.lnk"
SectionEnd
