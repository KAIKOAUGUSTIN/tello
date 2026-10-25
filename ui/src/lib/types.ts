// Espelho dos tipos de `tello-core` (crates/tello-core/src/model.rs e config.rs).

export type Protocol = 'sdk' | 'app';

export interface Sticks {
  roll: number;
  pitch: number;
  throttle: number;
  yaw: number;
}

export const NEUTRAL: Sticks = { roll: 0, pitch: 0, throttle: 0, yaw: 0 };

export type FlipDir =
  | 'forward'
  | 'back'
  | 'left'
  | 'right'
  | 'forward_left'
  | 'forward_right'
  | 'back_left'
  | 'back_right';

export type MoveDir = 'up' | 'down' | 'left' | 'right' | 'forward' | 'back';
export type SmartVideo = 'spin360' | 'circle' | 'up_and_away';

export type Action =
  | { type: 'takeoff' }
  | { type: 'land' }
  | { type: 'emergency' }
  | { type: 'stop' }
  | { type: 'flip'; dir: FlipDir }
  | { type: 'throw_and_go' }
  | { type: 'palm_land' }
  | { type: 'bounce'; on: boolean }
  | { type: 'smart_video'; mode: SmartVideo }
  | { type: 'take_photo' }
  | { type: 'stream_on' }
  | { type: 'stream_off' }
  | { type: 'move'; dir: MoveDir; cm: number }
  | { type: 'rotate'; clockwise: boolean; deg: number }
  | { type: 'set_speed'; cm_s: number }
  | { type: 'set_fast_mode'; on: boolean }
  | { type: 'set_max_height'; m: number }
  | { type: 'set_video_bitrate'; level: number }
  | { type: 'raw'; text: string };

export type Model = 'unknown' | 'tello' | 'tello_edu' | 'robo_master_tt';

export interface Capabilities {
  protocol: Protocol;
  model: Model;
  sdk_version: string | null;
  serial: string | null;
  firmware: string | null;
  flips: boolean;
  flips_diagonal: boolean;
  throw_and_go: boolean;
  palm_land: boolean;
  bounce: boolean;
  smart_video: boolean;
  hi_res_photo: boolean;
  fast_mode: boolean;
  max_height: boolean;
  video_bitrate: boolean;
  stop: boolean;
  mission_pads: boolean;
  raw_commands: boolean;
}

export interface MissionPad {
  id: number;
  x: number;
  y: number;
  z: number;
}

export interface Telemetry {
  battery: number | null;
  height_cm: number | null;
  tof_cm: number | null;
  baro_m: number | null;
  temp_low_c: number | null;
  temp_high_c: number | null;
  pitch_deg: number | null;
  roll_deg: number | null;
  yaw_deg: number | null;
  speed_x: number | null;
  speed_y: number | null;
  speed_z: number | null;
  accel: [number, number, number] | null;
  flight_time_s: number | null;
  wifi_strength: number | null;
  flying: boolean | null;
  mission_pad: MissionPad | null;
  battery_low: boolean | null;
  /** 0 = luz suficiente, 1 = pouca luz (modo App). */
  light_strength: number | null;
  fast_mode: boolean | null;
  wind: boolean | null;
  wifi_interference: number | null;
}

export type Condition = 'low_light' | 'wind' | 'weak_signal' | 'high_altitude';

export type WarningCode =
  | 'no_state'
  | 'no_video'
  | 'link_lost'
  | 'link_restored'
  | 'auto_land'
  | Condition
  | 'other';

export type DroneEvent =
  | { type: 'connected'; caps: Capabilities }
  | { type: 'disconnected'; reason: string }
  | { type: 'reconnecting'; attempt: number }
  | { type: 'telemetry'; data: Telemetry }
  | { type: 'warning'; code: WarningCode; message: string }
  | { type: 'conditions'; active: Condition[] }
  | ({ type: 'video_stats' } & VideoStats)
  | { type: 'photo_saved'; path: string }
  | { type: 'flight_report'; path: string | null; summary: FlightSummary };

export interface LogEntry {
  t_ms: number;
  dir: 'tx' | 'rx' | 'info';
  ch: 'cmd' | 'state' | 'video' | 'app' | 'core';
  text: string;
  hex?: string;
}

export interface Safety {
  stick_limit: number;
  max_height_cm: number | null;
  auto_land_battery: number | null;
  stick_timeout_ms: number;
}

/** Níveis do drone: 0 = automático, 1 = 1 Mbps, 2 = 1,5, 3 = 2, 4 = 3 e 5 = 4 Mbps. */
export interface VideoPolicy {
  /** Baixa a taxa quando o vídeo perde pacotes (modo App); `level` vira o teto. */
  adaptive: boolean;
  level: number;
}

export interface VideoStats {
  fps: number;
  kbps: number;
  /** Só no modo App. */
  loss_pct: number | null;
  level: number | null;
}

/** Resumo do voo, gerado no pouso junto com o relatório em HTML. */
export interface FlightSummary {
  started_unix_ms: number;
  duration_s: number;
  interrupted: boolean;
  battery_start: number | null;
  battery_end: number | null;
  max_height_cm: number | null;
  wifi_min: number | null;
  wifi_avg: number | null;
  video_loss_avg: number | null;
  temp_max_c: number | null;
  warnings: number;
  link_lost_s: number;
  /** Possíveis causas de instabilidade. */
  causes: string[];
}

export interface Config {
  drone_ip: string;
  safety: Safety;
  video: VideoPolicy;
  /** Fuso do usuário em minutos a partir do UTC, para a hora do relatório. */
  utc_offset_min: number;
}

export interface NetworkCheck {
  local_ip: string | null;
  same_subnet: boolean;
}

/** `os` vem de `std::env::consts::OS` ("windows", "macos", "linux", "android", "ios"). */
export interface PlatformInfo {
  os: string;
  mobile: boolean;
}

export interface RecordingResult {
  path: string;
  frames: number;
  seconds: number;
}

export interface VideoPacket {
  data: Uint8Array;
  key: boolean;
  hasSps: boolean;
  seq: number;
}
