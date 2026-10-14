/** Open Herd's mark: an "H" whose crossbar is an arch over a single dot. */
export default function Logo({ size = 34 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 34 34" aria-hidden="true">
      <defs>
        <linearGradient id="oh-logo" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#34D399" />
          <stop offset="1" stopColor="#0E7A5A" />
        </linearGradient>
      </defs>
      <rect width="34" height="34" rx="9" fill="url(#oh-logo)" />
      <path d="M10 9.5v15M24 9.5v15" stroke="#052E22" strokeWidth="3" strokeLinecap="round" fill="none" />
      <path d="M10 17c2.3-2.6 4.6-3.9 7-3.9s4.7 1.3 7 3.9" stroke="#052E22" strokeWidth="3" strokeLinecap="round" fill="none" />
      <circle cx="17" cy="21.5" r="1.9" fill="#052E22" />
    </svg>
  );
}
