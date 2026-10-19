; Ganchos do instalador NSIS.
;
; O Windows classifica o Wi-Fi do Tello como rede "Pública" e bloqueia UDP de
; entrada; sem esta regra a telemetria (8890) e o vídeo (11111/6038) não chegam.

!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Tello Pilot"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Tello Pilot" dir=in action=allow protocol=UDP profile=any enable=yes program="$INSTDIR\tello-pilot.exe"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Tello Pilot"'
!macroend
