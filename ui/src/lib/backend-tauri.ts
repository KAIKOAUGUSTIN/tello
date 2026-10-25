import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import { decodeVideoPacket, Emitter, type Backend, type FolderKind } from './backend';
import type { DroneEvent, LogEntry, RecordingResult, Safety, Sticks, VideoPacket, VideoPolicy } from './types';

export async function createTauriBackend(): Promise<Backend> {
  const events = new Emitter<DroneEvent>();
  const video = new Emitter<VideoPacket>();
  const logs = new Emitter<LogEntry[]>();
  const close = new Emitter<void>();

  await listen<DroneEvent>('drone-event', (e) => events.emit(e.payload));
  await listen<LogEntry[]>('drone-log', (e) => logs.emit(e.payload));
  await listen('confirm-close', () => close.emit());

  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (buf) => video.emit(decodeVideoPacket(buf));
  await invoke('set_video_channel', { channel });

  // Os sticks são enviados a ~30 Hz; erros aqui não devem travar a UI.
  const quiet = (p: Promise<unknown>) => p.catch(() => {});

  return {
    kind: 'tauri',
    platform: () => invoke('platform_info'),
    networkCheck: (ip) => invoke('network_check', { ip }),
    connect: (protocol, config) => invoke('connect', { protocol, config }),
    disconnect: () => invoke('disconnect'),
    action: (action) => invoke('action', { action }),
    setSticks: (sticks: Sticks) => void quiet(invoke('set_sticks', { sticks })),
    setSafety: (safety: Safety) => void quiet(invoke('set_safety', { safety })),
    setVideoPolicy: (policy: VideoPolicy) => void quiet(invoke('set_video_policy', { policy })),
    onEvent: (cb) => events.on(cb),
    onVideo: (cb) => video.on(cb),
    onLog: (cb) => logs.on(cb),
    onCloseRequest: (cb) => close.on(cb),
    logSnapshot: () => invoke('log_snapshot'),
    exportLog: (stamp) => invoke('export_log', { stamp }),
    saveSnapshot: (png, stamp) => invoke('save_snapshot', png, { headers: { 'x-stamp': stamp } }),
    startRecording: (stamp) => invoke('start_recording', { stamp }),
    stopRecording: () => invoke<RecordingResult>('stop_recording'),
    openFolder: (kind: FolderKind) => invoke('open_folder', { kind }),
    openReport: (path: string) => invoke('open_report', { path }),
    logNote: (text: string) => invoke('log_note', { text }),
    landAndQuit: () => invoke('land_and_quit'),
  };
}
