export function normalizePackageEntryPath(path: string): string {
  if (
    path.length === 0 ||
    path.startsWith("/") ||
    path.includes("\\") ||
    /^[A-Za-z]:/.test(path)
  ) {
    throw new Error(`Invalid package entry path: ${path}`);
  }

  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") throw new Error(`Invalid package entry path: ${path}`);
    if (part.startsWith(".")) throw new Error(`Invalid package entry path: ${path}`);
    parts.push(part);
  }

  const normalized = parts.join("/");
  if (
    normalized.length === 0 ||
    normalized === "sd" ||
    normalized.startsWith("sd/") ||
    normalized === "system" ||
    normalized.startsWith("system/") ||
    normalized.endsWith(".squid") ||
    normalized === "source-map.json" ||
    normalized.endsWith(".squid.zip")
  ) {
    throw new Error(`Invalid package entry path: ${path}`);
  }

  return normalized;
}
