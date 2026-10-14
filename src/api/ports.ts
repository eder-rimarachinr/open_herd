/** php-cgi's FastCGI port on Windows: 9000 + major*10 + minor (8.2 → 9082). */
export function phpPort(major: string): number | null {
  const [ma, mi] = major.split(".").map(Number);
  return Number.isInteger(ma) && Number.isInteger(mi) ? 9000 + ma * 10 + mi : null;
}
