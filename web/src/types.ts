// Generic state shape shared by all engine kinds (the kind-specific fields are
// read through the KindDef accessors in kinds.ts).
export interface AnyState {
  kind: string;
  time_s: number;
  mode: string;
  warnings: string[];
  logger_rows: number;
  logger_enabled: boolean;
  controls: Record<string, number | boolean>;
  environment: Record<string, number>;
  faults: Record<string, number | boolean>;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  [k: string]: any;
}

export interface AnySpec {
  kind?: string;
  name: string;
  manufacturer: string;
  application: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  [k: string]: any;
}

export type EngineTarget = number | "all";
