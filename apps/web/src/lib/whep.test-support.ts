/** Structural export for typecheck — whepUrl shape used by CameraTile. */
import { whepUrl } from "./whep";

export function expectedDemoWhep(base = "http://localhost:8889"): string {
  return whepUrl(base, "demo");
}

// Ensure build keeps whep module
void expectedDemoWhep;
