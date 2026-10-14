import styles from "./LogView.module.css";

// nginx: "2026/10/14 22:36:21 [emerg] 35628#23780: bind() …"
const NGINX_LINE = /^(\d{4}\/\d\d\/\d\d \d\d:\d\d:\d\d) \[(\w+)\] (?:\d+#\d+: )?(.*)$/;
// daemon: "[22:45:38] WARN: Nginx start warning: …" (level is optional)
const DAEMON_LINE = /^\[(\d\d:\d\d:\d\d)\] (?:(TRACE|DEBUG|INFO|WARN|ERROR): )?(.*)$/;

const LEVEL_CLASS: Record<string, string> = {
  emerg: "error", alert: "error", crit: "error", error: "error",
  warn: "warn", warning: "warn",
  notice: "note", info: "info", debug: "note", trace: "note",
};

type Line = { time?: string; level?: string; text: string };

function parse(line: string): Line {
  const n = NGINX_LINE.exec(line);
  if (n) return { time: n[1], level: n[2].toLowerCase(), text: n[3] };
  const d = DAEMON_LINE.exec(line);
  if (d) return { time: d[1], level: (d[2] ?? "info").toLowerCase(), text: d[3] };
  return { text: line };
}

/** A log rendered like a terminal: dim timestamps, colored levels. */
export default function LogView({ text, filter = "", className }: { text: string; filter?: string; className?: string }) {
  const q = filter.trim().toLowerCase();
  const lines = text.split("\n").filter((l) => l.trim() && (!q || l.toLowerCase().includes(q)));
  return (
    <div className={[styles.term, className ?? ""].join(" ")}>
      {lines.map((raw, i) => {
        const { time, level, text: msg } = parse(raw);
        return (
          <div key={i} className={styles.line}>
            {time && <span className={styles.time}>{time}</span>}
            {level && <span className={[styles.level, styles[LEVEL_CLASS[level] ?? "note"]].join(" ")}>{level}</span>}
            <span className={styles.msg}>{msg}</span>
          </div>
        );
      })}
    </div>
  );
}
