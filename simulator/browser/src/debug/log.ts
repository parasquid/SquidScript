export interface DebugEvent {
  at: string;
  scope: string;
  message: string;
  data?: Record<string, unknown>;
}

export function createDebugEvent(scope: string, message: string, data?: Record<string, unknown>): DebugEvent {
  return {
    at: new Date().toLocaleTimeString(),
    scope,
    message,
    data
  };
}

export function formatDebugData(data: Record<string, unknown> | undefined): string {
  if (!data) return "";
  return JSON.stringify(data);
}
