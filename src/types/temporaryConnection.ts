import type { ConnectionNetwork, SshConfig, SshRuntimeMode } from "@/types/global";

export type { SshRuntimeMode };

export interface TemporarySshLinkConfig extends SshConfig {
  protocol: "ssh";
  runtime_mode: SshRuntimeMode;
  backspace_mode: string;
  x11_forwarding: boolean;
  x11_display: string;
  proxy: null;
  proxy_jump: null;
  post_login: null;
}

export interface TemporaryTelnetLinkConfig {
  network?: ConnectionNetwork;
  encoding?: string;
  protocol: "telnet";
  name: string;
  host: string;
  port: number;
}

export interface TemporarySerialLinkConfig {
  protocol: "serial";
  name: string;
  portName: string;
  baudRate: number;
}

export type TemporaryLinkConfig =
  | TemporarySshLinkConfig
  | TemporaryTelnetLinkConfig
  | TemporarySerialLinkConfig;

export type TemporaryLinkProtocol = TemporaryLinkConfig["protocol"];
