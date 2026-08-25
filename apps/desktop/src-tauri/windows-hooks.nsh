!macro NSIS_HOOK_PREUNINSTALL
  ; The app registers this per-user Chrome host on first launch. Remove only
  ; Scrobble Bridge's own registration; preserve user data for reinstall.
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\com.scrobblebridge.host"
  Delete "$LOCALAPPDATA\Scrobble Bridge\NativeMessagingHosts\com.scrobblebridge.host.json"
  RMDir "$LOCALAPPDATA\Scrobble Bridge\NativeMessagingHosts"
!macroend
