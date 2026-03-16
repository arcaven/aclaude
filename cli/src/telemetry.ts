import type { TelemetryConfig } from "./config.js";

let initialized = false;

export function initTelemetry(config: TelemetryConfig): void {
  if (!config.enabled || initialized) return;

  // OTEL stub — when enabled, would initialize @opentelemetry/sdk-node
  // with OTLP exporter pointing at config.otel_endpoint.
  // For MVP, this is a no-op that logs the intent.
  if (config.otel_endpoint) {
    console.error(`[telemetry] OTEL endpoint configured: ${config.otel_endpoint} (stub — not yet wired)`);
  }

  initialized = true;
}

export function traceSpan(name: string, fn: () => void): void {
  // Stub: would create an OTEL span wrapping fn()
  fn();
}

export async function traceSpanAsync<T>(name: string, fn: () => Promise<T>): Promise<T> {
  // Stub: would create an OTEL span wrapping async fn()
  return fn();
}
